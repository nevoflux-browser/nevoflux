# Convention: salience-and-recency

Salience and recency are **two orthogonal axes** for surfacing pages without a search term. Adapted
from gbrain's salience-and-recency convention. Use this for personal-state and "what's notable"
questions.

## The two axes

- **Salience** = emotional weight + take density. *No time component.* "What matters / what's
  charged / what the user keeps returning to."
- **Recency** = age decay. *No mattering component.* "What's new / what changed lately."

They're independent: a page can be highly salient but old (an evergreen concept the user cares about),
or recent but low-salience (a routine daily note). Don't conflate them.

## Which tool for which question

| The user asks… | Use | Why |
| --- | --- | --- |
| "what's going on with me / what have I been thinking about" / "我最近在想什么" | `get_recent_transcripts` | raw conversation transcripts are the canonical source for the user's own state (local-only) |
| "what's notable / what's hot / anything crazy happening" / "最近有什么值得注意的" | `get_recent_salience` | ranks recent pages by emotional + activity salience; no search term needed |
| "what stood out / what changed / what's unusual" / "有什么异常" | `find_anomalies` | statistical anomalies by cohort (tag/type), with explanatory baselines |
| "what did I touch this week" / "我这周改了什么" | `list_pages sort=updated_desc` | pure recency, not salience |

**Do NOT run a semantic `query`/`search` for these.** Semantic search returns polished pages and
misses recent activity bursts — exactly what these questions are about.

## Reading the signal carefully

Words like "crazy", "notable", "big", or "厉害" often mean *difficult or emotionally charged*, not
*impressive*. Salience surfaces what's unusual or weighty — interpret results in that light and let
the user's own framing guide tone. Lead with the salient/anomalous pages, cite their slugs, and
offer to go deeper.

## Recency decay by prefix (when `recency: on`)

Per-prefix decay applies when recency is enabled on `query`: `daily/`, `chat/`, `media/x/` decay
aggressively; `concepts/`, `originals/`, `writing/` stay evergreen (recency component ≈ 0). Use
`recency: strong` only for "today / right now"; leave it off for canonical-truth questions.
