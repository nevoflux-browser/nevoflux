# Reference: media & documents → markdown, then capture

Convert files and media to text with the right tool, then run the normal capture spine. Converters
are external CLIs invoked with `run_command`. They run on Linux/macOS/Windows (Python/CLI tools) —
don't hardcode POSIX-only paths.

## Converter routing

| Input | Tool | Notes |
| --- | --- | --- |
| PDF — general | **markitdown** | fast, broad coverage |
| PDF — research paper / 财报 / complex layout & tables | **docling** | better structure/table/figure fidelity |
| .docx / .pptx / .xlsx / .csv / .epub / .zip | **markitdown** | office, ebooks, archives |
| image / screenshot | **LLM vision / OCR** (native) | markitdown as fallback |
| YouTube / Bilibili / video URL | **yt-dlp** → subtitles | download captions, then process the text |
| standalone audio | **markitdown** (built-in transcription) | else tell the user it's unsupported |

## Screenshots & social posts

- **Screenshot** → OCR/vision to extract, then route by what it *contains*: a tweet → social (below);
  an article → treat as a web article (`ingest.md`); a chart/data → extract the data points and
  describe the finding.
- **Social post (X, etc.)** → fetch the full thread + quoted posts, OCR any images, and **always**
  keep the direct link to the original (mandatory for the citation). File under `media/x/` or the
  relevant entity page. Cite `[Source: X/@{handle}, YYYY-MM-DD]({URL})`.

### PDF heuristic
Default to **markitdown**. Escalate to **docling** when the PDF is a research paper, financial
report, or is table/multi-column-heavy. Self-correction: if markitdown's tables come out garbled,
retry once with docling.

## Availability check + install hints

Before converting, confirm the tool exists; if missing, tell the user how to install rather than
failing opaquely:

```
run_command: markitdown --version   # or: docling --version / yt-dlp --version
# if not found:
#   pip install markitdown
#   pip install docling
#   pip install yt-dlp
```

## Invocations (examples)

```
# Document → markdown
run_command: markitdown "<path-or-url>" -o "<out.md>"
# Complex PDF → markdown (structure-preserving)
run_command: docling "<paper.pdf>" --to md --output "<outdir>"
# Video subtitles (no download of the video itself)
run_command: yt-dlp --write-auto-subs --write-subs --sub-lang "en,zh.*" --skip-download -o "<base>" "<video-url>"
```
Then `read_file` the produced `.md`/subtitle file to get the text.

## After conversion — capture

1. Write a **summary** page (highlights + metadata), not a raw transcript/text dump — `put_page`.
2. **Preserve the raw source**: `file_upload` (and/or `put_raw_data`) for provenance.
3. Extract entities; create reciprocal back-links + dated `timeline` entries; cite the source.
4. `sync_brain` if you added several pages and want the index current.
5. Confirm the slug(s).
