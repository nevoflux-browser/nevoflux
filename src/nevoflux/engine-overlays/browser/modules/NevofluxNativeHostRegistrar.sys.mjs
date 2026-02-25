/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

const lazy = {};
ChromeUtils.defineESModuleGetters(lazy, {
  ObjectUtils: "resource://gre/modules/ObjectUtils.sys.mjs",
});

export const NevofluxNativeHostRegistrar = {
  HOSTS: [
    { name: "com.nevoflux.agent", description: "NevoFlux AI Agent" },
    { name: "com.nevoflux.agent.mcp", description: "NevoFlux MCP Agent" },
  ],
  EXTENSION_ID: "agent@nevoflux.com",

  _getAgentBinaryFile() {
    let dir = Services.dirsvc.get("GreD", Ci.nsIFile);
    dir.append("distribution");
    dir.append("bin");
    dir.append(
      AppConstants.platform === "win" ? "nevoflux-agent.exe" : "nevoflux-agent"
    );
    return dir;
  },

  _getManifestFolder() {
    if (AppConstants.platform === "win") {
      return PathUtils.join(
        Services.dirsvc.get("AppData", Ci.nsIFile).path,
        "Mozilla",
        "NativeMessagingHosts"
      );
    } else if (AppConstants.platform === "macosx") {
      return PathUtils.join(
        Services.dirsvc.get("Home", Ci.nsIFile).path,
        "Library",
        "Application Support",
        "Mozilla",
        "NativeMessagingHosts"
      );
    }
    return PathUtils.join(
      Services.dirsvc.get("Home", Ci.nsIFile).path,
      ".mozilla",
      "native-messaging-hosts"
    );
  },

  async _writeManifest(folder, hostInfo, binaryPath) {
    let jsonContent = {
      name: hostInfo.name,
      description: hostInfo.description,
      path: binaryPath,
      type: "stdio",
      allowed_extensions: [this.EXTENSION_ID],
    };

    let manifestPath = PathUtils.join(folder, `${hostInfo.name}.json`);

    let correctFileExists = true;
    try {
      correctFileExists = lazy.ObjectUtils.deepEqual(
        await IOUtils.readJSON(manifestPath),
        jsonContent
      );
    } catch (e) {
      correctFileExists = false;
    }

    if (!correctFileExists) {
      await IOUtils.writeJSON(manifestPath, jsonContent);
    }

    if (AppConstants.platform === "win") {
      this._writeWindowsRegKey(hostInfo.name, manifestPath);
    }
  },

  _writeWindowsRegKey(hostName, manifestPath) {
    let wrk = Cc["@mozilla.org/windows-registry-key;1"].createInstance(
      Ci.nsIWindowsRegKey
    );
    try {
      let regPath = `Software\\Mozilla\\NativeMessagingHosts\\${hostName}`;
      try {
        wrk.create(
          wrk.ROOT_KEY_CURRENT_USER,
          regPath,
          wrk.ACCESS_ALL
        );
        if (wrk.readStringValue("") === manifestPath) {
          return;
        }
      } catch (e) {
        // Key doesn't exist or has no value; will be written below.
      }
      wrk.writeStringValue("", manifestPath);
    } catch (e) {
      console.error(
        `[NevoFlux] Failed to write registry key for ${hostName}:`,
        e
      );
    } finally {
      wrk.close();
    }
  },

  async ensureRegistered() {
    let binFile = this._getAgentBinaryFile();
    if (!binFile.exists()) {
      console.warn(
        "[NevoFlux] Agent binary not found at",
        binFile.path,
        "- skipping native host registration"
      );
      return;
    }

    let folder = this._getManifestFolder();
    await IOUtils.makeDirectory(folder, { createAncestors: true });

    for (let hostInfo of this.HOSTS) {
      try {
        await this._writeManifest(folder, hostInfo, binFile.path);
      } catch (e) {
        console.error(
          `[NevoFlux] Failed to write manifest for ${hostInfo.name}:`,
          e
        );
      }
    }
  },
};
