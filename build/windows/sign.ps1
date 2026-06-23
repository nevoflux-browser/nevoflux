# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# ==========================================================================
# NevoFlux Windows Code Signing Script
# ==========================================================================
# Adapted from Zen Browser's build/winsign/sign.ps1 for use with
# Certum Open Source Code Signing Certificate (SimplySign Cloud).
#
# Usage:
#   # Standard (SimplySign already connected, certificate loaded):
#   .\sign.ps1 -SignIdentity "Open Source Developer, YULIN GAN" -GithubRunId <run-id>
#
#   # With automatic SimplySign login (reads CERTUM_OTP_URI & CERTUM_USERID from env):
#   .\sign.ps1 -SignIdentity "Open Source Developer, YULIN GAN" -GithubRunId <run-id> -AutoConnect
#
#   # Use certificate thumbprint instead of CN (more reliable):
#   .\sign.ps1 -CertThumbprint "ABCDEF1234567890" -GithubRunId <run-id>
#
# Environment Variables (for -AutoConnect):
#   CERTUM_OTP_URI  - otpauth:// URI from SimplySign QR code
#   CERTUM_USERID   - SimplySign login email
#
# Prerequisites:
#   - SimplySign Desktop installed (or winget available for auto-install)
#   - Windows SDK (signtool.exe)
#   - GitHub CLI (gh) authenticated
#   - Node.js + npm
#   - mozilla-build (for l10n scripts)
#
# Note: NevoFlux uses a pinless SimplySign card — signing executes
# immediately without PIN prompts, ideal for batch/automated signing.
# ==========================================================================

param(
    [string]$SignIdentity,
    [string]$CertThumbprint,
    [string][Parameter(Mandatory=$true)]$GithubRunId,
    [switch]$AutoConnect,
    [switch]$SkipUpload
)

$ErrorActionPreference = "Stop"

# ==========================================================================
# Configuration - Change these for your fork
# ==========================================================================

$GITHUB_OWNER = "nevoflux-browser"
$GITHUB_REPO = "nevoflux"
$GITHUB_BINARIES_REPO = "nevoflux-windows-binaries"
$APP_NAME = "nevoflux"                    # Must match surfer.json binName
$INSTALLER_NAME = "$APP_NAME.installer"   # NSIS output name pattern

# Timestamp server - Certum's RFC 3161 server
# Using /tr (RFC 3161) instead of /t (legacy Authenticode) for better compatibility
$TIMESTAMP_URL = "http://time.certum.pl"

# ==========================================================================
# Validate parameters
# ==========================================================================

if (-not $SignIdentity -and -not $CertThumbprint) {
    throw "You must specify either -SignIdentity or -CertThumbprint.`n" +
          "  Example: .\sign.ps1 -SignIdentity 'Open Source Developer, YULIN GAN' -GithubRunId 12345`n" +
          "  Example: .\sign.ps1 -CertThumbprint 'ABCDEF...' -GithubRunId 12345"
}

# ==========================================================================
# SimplySign Auto-Connect (TOTP Automation)
# ==========================================================================

function Connect-SimplySign {
    <#
    .SYNOPSIS
    Automatically generate TOTP and login to SimplySign Desktop.
    Reads CERTUM_OTP_URI and CERTUM_USERID from environment variables.
    #>

    $OtpUri = $env:CERTUM_OTP_URI
    $UserId = $env:CERTUM_USERID

    if (-not $OtpUri) { throw "CERTUM_OTP_URI environment variable not set. Extract from SimplySign QR code." }
    if (-not $UserId) { throw "CERTUM_USERID environment variable not set. This is your SimplySign login email." }

    echo "=== Auto-connecting to SimplySign ==="

    # --- Parse otpauth:// URI ---
    # Format: otpauth://totp/SimplySign:user@email.com?secret=BASE32SECRET&issuer=SimplySign&digits=6&period=30
    $uri = [Uri]$OtpUri
    $queryParams = @{}
    foreach ($part in $uri.Query.TrimStart('?') -split '&') {
        $kv = $part -split '=', 2
        if ($kv.Count -eq 2) { $queryParams[$kv[0]] = [Uri]::UnescapeDataString($kv[1]) }
    }

    $Secret = $queryParams['secret']
    $Digits = if ($queryParams['digits']) { [int]$queryParams['digits'] } else { 6 }
    $Period = if ($queryParams['period']) { [int]$queryParams['period'] } else { 30 }

    if (-not $Secret) { throw "No 'secret' parameter found in OTP URI" }

    # --- Generate TOTP token ---
    Add-Type -Language CSharp @"
using System;
using System.Security.Cryptography;

public static class NevoFluxTotp {
    private const string B32_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    private static byte[] Base32Decode(string input) {
        input = input.TrimEnd('=').ToUpperInvariant();
        byte[] output = new byte[input.Length * 5 / 8];
        int bitBuffer = 0, bitsInBuffer = 0, outputIndex = 0;
        foreach (char c in input) {
            int val = B32_ALPHABET.IndexOf(c);
            if (val < 0) throw new ArgumentException("Invalid Base32 character: " + c);
            bitBuffer = (bitBuffer << 5) | val;
            bitsInBuffer += 5;
            if (bitsInBuffer >= 8) {
                output[outputIndex++] = (byte)(bitBuffer >> (bitsInBuffer - 8));
                bitsInBuffer -= 8;
            }
        }
        return output;
    }

    public static string Generate(string secret, int digits, int period) {
        byte[] key = Base32Decode(secret);
        long counter = DateTimeOffset.UtcNow.ToUnixTimeSeconds() / period;
        byte[] counterBytes = BitConverter.GetBytes(counter);
        if (BitConverter.IsLittleEndian) Array.Reverse(counterBytes);

        using (var hmac = new HMACSHA1(key)) {
            byte[] hash = hmac.ComputeHash(counterBytes);
            int offset = hash[hash.Length - 1] & 0x0F;
            int binary = ((hash[offset] & 0x7F) << 24) |
                         ((hash[offset + 1] & 0xFF) << 16) |
                         ((hash[offset + 2] & 0xFF) << 8)  |
                          (hash[offset + 3] & 0xFF);
            return (binary % (int)Math.Pow(10, digits)).ToString(new string('0', digits));
        }
    }
}
"@

    $totp = [NevoFluxTotp]::Generate($Secret, $Digits, $Period)
    echo "TOTP token generated"

    # --- Find SimplySign Desktop ---
    # Download from: https://support.certum.eu/en/cert-offer-software-and-libraries/
    $ssPath = @(
        "C:\Program Files\Certum\SimplySign Desktop\SimplySignDesktop.exe",
        "C:\Program Files (x86)\Certum\SimplySign Desktop\SimplySignDesktop.exe",
        "${env:ProgramFiles}\Certum\proCertum SmartSign\SimplySignDesktop.exe",
        "${env:LOCALAPPDATA}\Programs\SimplySign Desktop\SimplySignDesktop.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1

    if (-not $ssPath) {
        echo "SimplySign Desktop not found. Attempting winget install..."
        winget install Certum.SmartSignSimplySignDesktop --accept-source-agreements --accept-package-agreements --silent 2>$null
        Start-Sleep -Seconds 10
        $ssPath = Get-ChildItem "C:\Program Files*" -Recurse -Filter "SimplySignDesktop.exe" -ErrorAction SilentlyContinue |
            Select-Object -First 1 -ExpandProperty FullName
        if (-not $ssPath) {
            throw "Could not find or install SimplySign Desktop. Install manually from https://support.certum.eu/"
        }
    }

    echo "Launching SimplySign Desktop: $ssPath"
    $proc = Start-Process -FilePath $ssPath -PassThru
    echo "Waiting for SimplySign Desktop to start (icon appears in system tray)..."
    Start-Sleep -Seconds 8

    # --- Trigger "Connect to SimplySign" via tray icon right-click ---
    # The login dialog ("Logowanie") has two fields:
    #   1. Identyfikator: your SimplySign registered email
    #   2. Token: the 6-digit TOTP from SimplySign mobile app
    # Then click Ok to connect.
    #
    # Since we can't easily right-click the tray icon via SendKeys, we use
    # AppActivate to find the login window. If SimplySign auto-opens the
    # login dialog, we fill it. Otherwise, we try to trigger it.

    $wshell = New-Object -ComObject WScript.Shell

    # Wait for the login window ("Logowanie" / "SimplySign Desktop")
    $focused = $false
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        # Try various window title patterns (depends on language/version)
        $focused = $wshell.AppActivate('Logowanie')
        if (-not $focused) { $focused = $wshell.AppActivate('SimplySign Desktop') }
        if (-not $focused) { $focused = $wshell.AppActivate($proc.Id) }
        if ($focused) { break }
        Start-Sleep -Milliseconds 500
    }

    if (-not $focused) {
        echo "WARNING: Could not find SimplySign login window."
        echo "The app may already be connected, or the login dialog didn't auto-open."
        echo "If not connected, please right-click the SimplySign tray icon → 'Connect to SimplySign'"
        echo "and log in manually within 60 seconds..."
        Start-Sleep -Seconds 30
    } else {
        Start-Sleep -Milliseconds 500

        # The login dialog ("Logowanie") layout per Certum docs (Figure 4):
        #   Field 1: "Identyfikator" (email) - should be the first focused field
        #   Field 2: "Token" (TOTP code)
        #   Button: "Ok" / "Anuluj" (Cancel)
        #
        # SendKeys: type email → Tab to Token field → type TOTP → Enter (= Ok)
        $wshell.SendKeys("$UserId{TAB}$totp{ENTER}")
        echo "Credentials sent: Identyfikator=$UserId, Token=****** (6 digits)"
    }

    # --- Wait for certificate to become available ---
    echo "Waiting for certificate to become available in certificate store..."
    $maxWait = 60
    $certFound = $false

    for ($i = 0; $i -lt $maxWait; $i += 3) {
        Start-Sleep -Seconds 3
        $certs = Get-ChildItem Cert:\CurrentUser\My -ErrorAction SilentlyContinue | Where-Object {
            $_.Subject -like "*Open Source Developer*" -or
            ($SignIdentity -and $_.Subject -like "*$SignIdentity*")
        }
        if ($certs.Count -gt 0) {
            $certFound = $true
            echo "Certificate loaded successfully!"
            echo "  Subject: $($certs[0].Subject)"
            echo "  Thumbprint: $($certs[0].Thumbprint)"
            echo "  Expires: $($certs[0].NotAfter)"

            if (-not $CertThumbprint) {
                $script:CertThumbprint = $certs[0].Thumbprint
                echo "  Using thumbprint for signing: $CertThumbprint"
            }
            break
        }
    }

    if (-not $certFound) {
        echo "WARNING: Certificate not detected in store after ${maxWait}s."
        echo "Proceeding anyway - signing may fail if SimplySign isn't fully connected."
    }
}

# ==========================================================================
# Helper: Build signtool arguments
# ==========================================================================

function Get-SignToolArgs {
    <#
    Returns the base signtool arguments for signing.
    Per Certum official documentation (Page 8):
      signtool sign /sha1 "[thumbprint]" /tr [timestamp] /td [digest] /fd [digest] /v "[file]"
    Uses /sha1 (thumbprint) if available - this is the recommended method.
    Falls back to /n (CN identity string) for compatibility with upstream Zen Browser.
    Uses /tr (RFC 3161 timestamp) instead of /t (legacy Authenticode) for better long-term validity.
    #>
    $args = @()

    if ($CertThumbprint) {
        $args += "/sha1", $CertThumbprint
    } elseif ($SignIdentity) {
        $args += "/n", $SignIdentity
    }

    # RFC 3161 timestamp (/tr) is preferred over legacy Authenticode (/t)
    # /td sha256 specifies the digest algorithm for the timestamp
    $args += "/tr", $TIMESTAMP_URL
    $args += "/td", "sha256"
    $args += "/fd", "sha256"
    $args += "/v"

    return $args
}

# ==========================================================================
# Main Script
# ==========================================================================

echo ""
echo "============================================================"
echo "  NevoFlux Windows Code Signing"
echo "============================================================"
echo ""

# --- Step 0: Auto-connect SimplySign if requested ---

if ($AutoConnect) {
    Connect-SimplySign
}

# --- Step 1: Prepare environment ---

echo "Preparing environment"
git pull origin main --recurse
mkdir windsign-temp -ErrorAction SilentlyContinue

# --- Step 2: Parallel downloads ---

Start-Job -Name "DownloadGitl10n" -ScriptBlock {
    param($PWD)
    cd $PWD
    $env:ZEN_L10N_CURR_DIR=[regex]::replace($PWD, "^([A-Z]):", { "/" + $args.value.Substring(0, 1).toLower() }) -replace "\\", "/"
    C:\mozilla-build\start-shell.bat $PWD\scripts\download-language-packs.sh
    echo "Fetched l10n and Firefox's one"
} -Verbose -ArgumentList $PWD -Debug

Start-Job -Name "SurferInit" -ScriptBlock {
    param($PWD)
    cd $PWD
    npm run import -- --verbose
    $surferJson = Get-Content surfer.json | ConvertFrom-Json
    $version = $surferJson.brands.release.release.displayVersion
    npm run ci -- $version
} -Verbose -ArgumentList $PWD -Debug

# --- Step 3: Download build artifacts ---

echo "Downloading artifacts info"
$artifactsInfo = gh api repos/$GITHUB_OWNER/$GITHUB_REPO/actions/runs/$GithubRunId/artifacts
$token = gh auth token

function New-TemporaryDirectory {
    $tmp = [System.IO.Path]::GetTempPath()
    $name = (New-Guid).ToString("N")
    New-Item -ItemType Directory -Path (Join-Path $tmp $name)
}

function DownloadFile($url, $targetFile) {
    $uri = New-Object "System.Uri" "$url"
    $request = [System.Net.HttpWebRequest]::Create($uri)
    $request.UserAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36"
    $request.Headers.Add("Authorization", "Bearer $token")
    $response = $request.GetResponse()
    $totalLength = [System.Math]::Floor($response.get_ContentLength()/1024)
    $responseStream = $response.GetResponseStream()
    $targetStream = New-Object -TypeName System.IO.FileStream -ArgumentList $targetFile, Create
    $buffer = new-object byte[] 10KB
    $count = $responseStream.Read($buffer,0,$buffer.length)
    $downloadedBytes = $count

    while ($count -gt 0) {
        $targetStream.Write($buffer, 0, $count)
        $count = $responseStream.Read($buffer,0,$buffer.length)
        $downloadedBytes = $downloadedBytes + $count
        Write-Progress -activity "Downloading file '$($url.split('/') | Select -Last 1)'" -status "Downloaded ($([System.Math]::Floor($downloadedBytes/1024))K of $($totalLength)K): " -PercentComplete ((([System.Math]::Floor($downloadedBytes/1024)) / $totalLength)  * 100)
    }

    Write-Progress -activity "Finished downloading file '$($url.split('/') | Select -Last 1)'"

    $targetStream.Flush()
    $targetStream.Close()
    $targetStream.Dispose()
    $responseStream.Dispose()
}

function DownloadArtifacts($name) {
    echo "Downloading artifacts for $name"
    # ======================================================================
    # IMPORTANT: Change artifact name pattern to match YOUR workflow output
    # Zen uses: windows-x64-obj-{arch}
    # Adjust if your workflow names artifacts differently
    # ======================================================================
    $artifactName = "windows-x64-obj-$name"
    $artifactUrl = $($artifactsInfo | jq -r --arg NAME $artifactName '.artifacts[] | select(.name == $NAME) | .archive_download_url')

    if (-not $artifactUrl -or $artifactUrl -eq "null") {
        # Try alternative naming patterns
        $altNames = @(
            "nevoflux-windows-obj-$name",
            "nevoflux-$name-obj",
            "windows-$name-obj"
        )
        foreach ($altName in $altNames) {
            $artifactUrl = $($artifactsInfo | jq -r --arg NAME $altName '.artifacts[] | select(.name == $NAME) | .archive_download_url')
            if ($artifactUrl -and $artifactUrl -ne "null") {
                $artifactName = $altName
                echo "Found artifact with alternative name: $altName"
                break
            }
        }
    }

    if (-not $artifactUrl -or $artifactUrl -eq "null") {
        throw "Could not find artifact matching pattern for '$name'. Available artifacts:`n$(($artifactsInfo | jq -r '.artifacts[].name') -join "`n")"
    }

    echo "Artifact URL: $artifactUrl"

    $outputPath = "$PWD\windsign-temp\windows-x64-obj-$name"
    $tempDir = New-TemporaryDirectory
    $tempFile = Join-Path $tempDir "artifact-$($name).zip"

    echo "Downloading artifact to $tempFile"
    DownloadFile $artifactUrl $tempFile

    Start-Job -Name "UnzipArtifact$name" -ScriptBlock {
        param($tempFile, $outputPath)
        echo "Unzipping artifact to $outputPath"
        Expand-Archive -Path $tempFile -DestinationPath $outputPath -Force
        echo "Unzipped artifact to $outputPath"
    } -ArgumentList $tempFile, $outputPath -Verbose -Debug
}

DownloadArtifacts arm64
DownloadArtifacts x86_64

Wait-Job -Name "UnzipArtifactarm64"
Wait-Job -Name "UnzipArtifactx86_64"

# --- Pause: Ensure SimplySign is connected before signing ---

if (-not $AutoConnect) {
    echo ""
    echo "============================================================"
    echo "  Downloads complete. Ready to sign."
    echo ""
    echo "  Please ensure SimplySign Desktop is connected:"
    echo "    1. Right-click SimplySign tray icon"
    echo "    2. Connect to SimplySign (enter email + token)"
    echo "    3. Wait for 'Connected' notification"
    echo "============================================================"
    echo ""

    do {
        $answer = Read-Host "SimplySign connected? Press [Y] to start signing"
    } while ($answer -notin @('Y', 'y'))
}

# --- Step 4: Sign all binaries (exe + dll) ---

mkdir engine\obj-x86_64-pc-windows-msvc\ -ErrorAction SilentlyContinue

echo ""
echo "=== Signing all binaries ==="
echo ""

$files = Get-ChildItem windsign-temp\windows-x64-obj-x86_64\ -Recurse -Include *.exe
$files += Get-ChildItem windsign-temp\windows-x64-obj-x86_64\ -Recurse -Include *.dll
$files += Get-ChildItem windsign-temp\windows-x64-obj-arm64\ -Recurse -Include *.exe
$files += Get-ChildItem windsign-temp\windows-x64-obj-arm64\ -Recurse -Include *.dll

echo "Found $($files.Count) files to sign"

# Build signtool command
# Per Certum official docs (Page 8-10):
#   signtool sign /sha1 "[thumbprint]" /tr http://time.certum.pl /td sha256 /fd sha256 /v "[file]"
# Batch signing: pass all files to one signtool call for efficiency.
# NevoFlux uses a pinless SimplySign card - signing executes immediately without PIN prompt.
$signArgs = Get-SignToolArgs
signtool.exe sign @signArgs $files

if ($LASTEXITCODE -ne 0) {
    echo "WARNING: Some files may have failed to sign. Check output above."
    echo "Common causes:"
    echo "  - SimplySign not connected (run with -AutoConnect or connect manually)"
    echo "  - Certificate thumbprint/identity mismatch"
    echo "  - Timestamp server temporarily unavailable"

    # Retry once with a delay (timestamp server can be flaky)
    echo "Retrying in 5 seconds..."
    Start-Sleep -Seconds 5
    signtool.exe sign @signArgs $files
    if ($LASTEXITCODE -ne 0) {
        throw "Signing failed after retry. Please check SimplySign connection and certificate."
    }
}

echo "All binaries signed successfully"

# --- Step 5: Wait for parallel jobs ---

$env:ZEN_RELEASE = "true"
$env:SURFER_SIGNING_MODE = "true"
$env:SCCACHE_GHA_ENABLED = "false"
Wait-Job -Name "SurferInit"
Wait-Job -Name "DownloadGitl10n"

# --- Step 6: Package and organize per architecture ---

function SignAndPackage($name) {
    echo ""
    echo "=== Processing architecture: $name ==="
    echo ""

    rmdir .\dist -Recurse -ErrorAction SilentlyContinue

    $objName = $name
    if ($name -eq "arm64") {
        $objName = "aarch64"
    }

    echo "Removing old obj dir"
    rmdir engine\obj-$objName-pc-windows-msvc\ -Recurse -ErrorAction SilentlyContinue

    echo "Creating new obj dir"
    cp windsign-temp\windows-x64-obj-$name engine\obj-$objName-pc-windows-msvc\ -Recurse

    echo "Copying setup.exe into obj dir"
    $env:ZEN_SETUP_EXE_PATH = "$PWD\windsign-temp\windows-x64-obj-$name\browser\installer\windows\instgen\setup.exe"

    # ======================================================================
    # MSVC Redistributables path
    # Adjust the version number (14.38.33135) to match your VS installation
    # ======================================================================
    if ($name -eq "arm64") {
        $env:WIN32_REDIST_DIR = "$PWD\win-cross\vs2026\VC\Redist\MSVC\14.50.35710\arm64\Microsoft.VC145.CRT"
    } else {
        $env:WIN32_REDIST_DIR = "$PWD\win-cross\vs2026\VC\Redist\MSVC\14.50.35710\x64\Microsoft.VC145.CRT"
    }

    $env:MAR = "..\build\winsign\mar.exe"

    if ($name -eq "arm64") {
        $env:SURFER_COMPAT = "aarch64"
    } else {
        $env:SURFER_COMPAT = "x86_64"
    }
    echo "Compat Mode? $env:SURFER_COMPAT"

    # Reconfigure - necessary because artifacts come from a Linux cross-compile
    # environment and the build system needs to detect we're on Windows now
    cd .\engine
    echo "Configuring for $name"
    .\mach configure
    cd ..

    echo "Packaging $name"
    npm run package -- --verbose

    # ======================================================================
    # Organize output files
    # Structure matches what the release workflow expects:
    #   windows-x64-signed-{arch}/
    #     ├── windows[-arm64].mar
    #     ├── nevoflux.installer[-arm64].exe
    #     └── update_manifest/
    # ======================================================================
    echo "Creating output structure for $name"
    rm .\windsign-temp\windows-x64-signed-$name -Recurse -ErrorAction SilentlyContinue
    mkdir windsign-temp\windows-x64-signed-$name

    # Move the MAR update file
    echo "Moving MAR for $name"
    if ($name -eq "arm64") {
        mv .\dist\output.mar windsign-temp\windows-x64-signed-$name\windows-$name.mar
    } else {
        mv .\dist\output.mar windsign-temp\windows-x64-signed-$name\windows.mar
    }

    # Move the NSIS installer
    # ======================================================================
    # IMPORTANT: The installer filename is derived from surfer.json binName.
    # If your surfer.json sets binName to "nevoflux", the output will be
    # "nevoflux.installer.exe". Adjust the pattern below if different.
    # ======================================================================
    echo "Moving installer for $name"
    $installerFile = Get-ChildItem .\dist\ -Filter "$APP_NAME.installer*.exe" -ErrorAction SilentlyContinue |
        Select-Object -First 1

    if (-not $installerFile) {
        # Fallback: look for any installer exe
        $installerFile = Get-ChildItem .\dist\ -Filter "*.installer*.exe" -ErrorAction SilentlyContinue |
            Select-Object -First 1
    }

    if (-not $installerFile) {
        throw "Could not find installer exe in .\dist\. Contents: $(Get-ChildItem .\dist\ | Select-Object -ExpandProperty Name)"
    }

    echo "Found installer: $($installerFile.Name)"

    if ($name -eq "arm64") {
        mv $installerFile.FullName windsign-temp\windows-x64-signed-$name\$APP_NAME.installer-$name.exe
    } else {
        mv $installerFile.FullName windsign-temp\windows-x64-signed-$name\$APP_NAME.installer.exe
    }

    # Move the update manifests
    mv .\dist\update\. windsign-temp\windows-x64-signed-$name\update_manifest

    # Stage for commit to binaries repo
    if (-not $SkipUpload) {
        rmdir .\windsign-temp\$GITHUB_BINARIES_REPO\windows-x64-signed-$name -Recurse -ErrorAction SilentlyContinue
        mv windsign-temp\windows-x64-signed-$name .\windsign-temp\$GITHUB_BINARIES_REPO -Force
    }

    rmdir engine\obj-$objName-pc-windows-msvc\ -Recurse -ErrorAction SilentlyContinue

    echo "Finished $name"
}

SignAndPackage arm64
SignAndPackage x86_64

# --- Step 7: Sign the installer executables ---

echo ""
echo "=== Signing installers ==="
echo ""

if (-not $SkipUpload) {
    $installerFiles = Get-ChildItem .\windsign-temp\$GITHUB_BINARIES_REPO -Recurse -Include *.exe
} else {
    $installerFiles = Get-ChildItem .\windsign-temp\windows-x64-signed-* -Recurse -Include *.exe
}

echo "Signing $($installerFiles.Count) installer file(s)"
$signArgs = Get-SignToolArgs
signtool.exe sign @signArgs $installerFiles

if ($LASTEXITCODE -ne 0) {
    echo "Retrying installer signing..."
    Start-Sleep -Seconds 5
    signtool.exe sign @signArgs $installerFiles
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to sign installer executables"
    }
}

# Verify signatures
echo ""
echo "=== Verifying signatures ==="
echo ""
foreach ($file in $installerFiles) {
    echo "Verifying: $($file.Name)"
    # Per Certum docs: signtool verify /pa /all [file]
    signtool.exe verify /pa /all $file.FullName
    if ($LASTEXITCODE -eq 0) {
        echo "  OK"
    } else {
        echo "  VERIFICATION FAILED for $($file.Name)"
    }
}

# --- Step 8: Upload to binaries repo ---

echo ""
echo "=== All artifacts signed and packaged, ready for release! ==="

if (-not $SkipUpload) {
    if (-not (Test-Path "windsign-temp\$GITHUB_BINARIES_REPO\.git")) {
        echo "Cloning $GITHUB_BINARIES_REPO..."
        git clone https://github.com/$GITHUB_OWNER/$GITHUB_BINARIES_REPO.git windsign-temp\$GITHUB_BINARIES_REPO 2>$null
        if ($LASTEXITCODE -ne 0) {
            echo "WARNING: Could not clone $GITHUB_BINARIES_REPO."
            echo "If the repo doesn't exist yet, create it on GitHub first."
            echo "Signed files are in: windsign-temp\windows-x64-signed-*"
            $SkipUpload = $true
        }
    }

    if (-not $SkipUpload) {
        echo "Committing changes to $GITHUB_BINARIES_REPO"
        cd windsign-temp\$GITHUB_BINARIES_REPO
        git add .
        git commit -m "Sign and package windows artifacts (run: $GithubRunId)"
        git push
        cd ..\..
        echo "Pushed to $GITHUB_BINARIES_REPO successfully"
    }
}

# --- Step 9: Cleanup ---

echo ""
echo "============================================================"
echo "  All done!"
echo "  Both x86_64 and arm64 artifacts are signed and packaged."
echo "============================================================"
echo ""

if (-not $SkipUpload) {
    echo "Signed artifacts pushed to: https://github.com/$GITHUB_OWNER/$GITHUB_BINARIES_REPO"
} else {
    echo "Signed artifacts location:"
    echo "  windsign-temp\windows-x64-signed-x86_64\"
    echo "  windsign-temp\windows-x64-signed-arm64\"
}

echo ""
Read-Host "Press Enter to clean up temp files and continue"

echo "Cleaning up"
rmdir windsign-temp\windows-x64-obj-x86_64 -Recurse -ErrorAction SilentlyContinue
rmdir windsign-temp\windows-x64-obj-arm64 -Recurse -ErrorAction SilentlyContinue

echo "Done!"
