# Convention: brain-routing (RESOLVER)

Maps a user intent to the skill that handles it and the primary gbrain tool to reach for. When more
than one could apply, use the disambiguation rules at the bottom.

## Intent → skill → primary tool

### brain-recall (READ)
| Intent (EN / 中文) | Primary tool |
| --- | --- |
| "what do I know about X" / "我的知识库里有没有 X" | `query` → `get_page` |
| "find my note on…" / "我之前记过…" | `search` / `resolve_slugs` → `get_page` |
| "what's going on with me / what's notable" / "最近有什么" | `get_recent_salience` / `find_anomalies` |
| "what did I touch this week" / "我这周改了什么" | `list_pages sort=updated_desc` |
| "catch me up / daily briefing" / "日报" | briefing workflow (recall) |
| "what links to / who works at / connected to" / "和这个相关的" | `get_links` / `get_backlinks`; shallow `traverse_graph` (link_type, depth<=2) |

### brain-capture (WRITE / INGEST)
| Intent | Primary tool |
| --- | --- |
| "save this / remember that" / "记到知识库 / 存到…" | `put_page` (+ `add_timeline_entry`) |
| "ingest this URL/article/text" / "收录这个" | ingest workflow → `put_page` |
| "process this PDF/doc/video" / "处理这个文件/视频" | media workflow (markitdown/docling/yt-dlp) |
| "process this meeting" / "整理这个会议记录" | meeting workflow |
| "enrich this / look them up" / "补充资料" | enrich workflow (on request) |
| "remember I prefer…" (a fact) | `extract_facts` |

### brain-think (ANALYZE, + persist on request)
| Intent | Primary tool |
| --- | --- |
| "connect the dots / synthesize" / "帮我梳理" | `think` |
| "who knows about X / who should I talk to" / "谁懂 X" | `find_experts` |
| "how has X trended / is X consistent" / "X 的趋势" | `find_trajectory` |
| "how are these related" (open / deep multi-hop) / "关系图" | `traverse_graph` (deep) |
| "what's my track record / am I calibrated" | `takes_scorecard` / `takes_calibration` |
| "build my concept map" / "整理我的概念" | concept-map workflow → `put_page` |
| "read X through the lens of my problem Y" | strategic-reading workflow → `put_page` |

### brain-care (DIAGNOSE / SYNC / RECOVER)
| Intent | Primary tool |
| --- | --- |
| "is my brain healthy" / "知识库健康吗" | `get_health` / `run_doctor` / `get_stats` |
| "what's inconsistent / contradictions" / "有没有矛盾" | `find_contradictions` (report only) |
| "what's orphaned / unlinked" / "孤立页面" | `find_orphans` |
| "sync my brain" / "同步" | `sync_brain` (suggest `dry_run` first) |
| "undo that delete / restore a page" / "恢复" | `restore_page` |
| "revert this page" / "回退到上个版本" | `get_versions` → `revert_version` |

## Disambiguation rules

1. **Verb decides.** "save / 记 / 存 / ingest" → capture. "what do I know / 我知道" → recall.
   "connect / synthesize / who knows / 趋势" → think. "healthy / sync / restore / 矛盾" → care.
2. **Read vs write.** A request that only *answers a question* stays in recall/think (read).
   A request that *changes the brain* goes to capture (write) or care (recover).
3. **Simple vs deep.** A direct lookup → recall (`query`/`get_page`). A multi-hop / cross-page /
   "why / how related / what's the pattern" question → think (`think`/`traverse_graph`).
4. **Specificity wins.** A narrower match beats a broader one (e.g. "build my concept map" → think's
   concept-map workflow, not a generic recall).
5. **Personal-state ≠ search.** "what's going on with me / 最近 / anything notable" → salience /
   anomalies / transcripts, never semantic search.
6. **When ambiguous, ask** which the user wants rather than guessing across the read/write boundary.
