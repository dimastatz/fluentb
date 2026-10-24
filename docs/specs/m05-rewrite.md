# M5 — Rewrite

## Summary

Implement the Rewrite hotkey: read the selected text in the focused app, rewrite it into clean, fluent, correctly styled text with a **local** language model, and replace the selection with the result. Like dictation, rewrite never sends user content off the machine.

## Goals

- One hotkey turns rough text into polished text in any app.
- The rewrite runs entirely on-device; there are no network calls with user text.
- The user's meaning, language, and formatting such as lists and line breaks are preserved.
- The original text is never lost if anything fails.

## Non-goals

- Cloud or remote rewrite engines. These are out of scope permanently, not just for M5.
- Interactive diff or preview UI (possible later).
- Multi-turn or chat-style editing.
- Model download (M6). M5 loads a GGUF model from a configured local path.

## User stories

- As a user, I select a sloppy Slack draft, press Rewrite, and it becomes a clear message.
- As a user writing in Hebrew or German, the rewrite stays in my language.
- As a user, I can rewrite on a plane with no network connection.
- As a user, if the rewrite fails, my original text is still there.

## Requirements

### Functional

1. **Read selection** (macOS)
   - Primary: `AXUIElement` focused element → `kAXSelectedTextAttribute`.
   - Fallback for apps without AX text (Electron, browsers): save the clipboard, send Cmd+C, read it, then restore the clipboard.
   - Return `None` when the selection is empty.
2. **Rewrite engine** — a `RewriteEngine` trait behind `Facade::rewrite`:
   - `fn rewrite(&self, text: &str, style: Style) -> Result<String>`
   - The single implementation is `LlamaEngine`: llama.cpp through the `llama-cpp-2` crate, running a GGUF instruct model with Metal acceleration on macOS.
   - The trait exists for testing (`FakeRewriteEngine`) and for swapping local models, not for remote engines.
3. **Model**
   - Default candidates, decided by the evaluation below:
     - Qwen3-4B Q4_K_M (~2.5 GB), thinking mode disabled. Apache 2.0, so preferred if quality is comparable.
     - Gemma 3 4B-it Q4_K_M (~2.5 GB), strong at multilingual rewriting, under the Gemma Terms of Use.
   - Light option for low-RAM machines: Qwen3-1.7B Q4_K_M (~1.1 GB), thinking mode disabled.
   - Also evaluate Llama 3.2 3B and Phi-4-mini.
   - Models under ~1.5B parameters are out: they drift from the original meaning, especially in non-English text.
   - Every registry model needs a licence that allows fluentb to download and run it for any user, including commercial use. Non-commercial licences, such as Qwen2.5-3B's Qwen Research licence, are excluded. Licences are listed below.
   - Config: `[rewrite] model = "qwen3-4b"`, chosen from the M6 model registry.
4. **Memory policy** — load the model on the first rewrite, keep it warm, and unload it after a configurable idle period (default 10 minutes) so idle RAM stays low. `rewrite.keep_loaded = true` disables unloading.
5. **Prompt** — built by a pure function `fn build_prompt(text, style) -> Prompt` using the model's chat template, with these rules:
   - Fix grammar, spelling, and punctuation; improve flow.
   - Keep meaning, facts, names, URLs, code spans, emoji, and the language.
   - Apply the configured style (neutral, casual, formal, concise).
   - Return only the rewritten text, with no preamble.
6. **Decoding** — low temperature (≈ 0.2) and a token limit derived from the input length (about 1.5× the input tokens plus a margin) so the output can't run away.
7. **Post-processing** — a pure `fn clean_output(raw) -> String` strips quotes, preambles ("Here is…"), and trailing whitespace. If the result is empty, or much longer than the input (over 3×), it's treated as a failure and nothing is inserted.
8. **Replace** — `insert_text` replaces the still-active selection. If the focus or selection changed during the rewrite, the app does not insert. It copies the result to the clipboard and notifies "Rewrite copied to clipboard" instead.
9. **Limits** — reject selections over a configurable length (default 4,000 characters) with a notification.

### Non-functional

- **Privacy:** no network crate in the rewrite path. The M3 `cargo tree` check is extended to cover rewrite modules.
- **Latency** on an Apple Silicon Mac (M1, 16 GB) with the default model:
  - warm: ≤ 3 s for a 500-character paragraph
  - cold, including model load: ≤ 6 s
- **Memory:** the model is resident only while warm. Document the peak RSS of each registry model.
- Rewrite text is never logged.
- Inference runs off the event thread so the tray and hotkeys stay responsive.

## Model evaluation

Before fixing the default, compare 2–3 candidate models on a fixed set of about 50 sample inputs (English, Hebrew, German, Russian; chat messages, emails, code comments) for:

- meaning preserved and no hallucinated content
- the language kept the same
- style rules followed
- latency and peak RAM
- licence, as a tie-breaker: when quality is close, prefer the more permissive licence

Candidate licences (confirm against each model card before adding it to the registry):

| Model | Licence | Notes |
| --- | --- | --- |
| Qwen3-1.7B, Qwen3-4B | Apache 2.0 | Permissive, no extra terms |
| Phi-4-mini | MIT | Permissive |
| Gemma 3 4B-it | Gemma Terms of Use | Redistribution allowed; a usage policy applies |
| Llama 3.2 3B | Llama 3.2 Community License | Attribution required; extra terms above 700M monthly users |
| Qwen2.5-3B | Qwen Research | Non-commercial: excluded |

The evaluation script and inputs live in `eval/rewrite/` and run manually. They are not part of CI.

## Testing

- Pure tests for `build_prompt` (each style and the chat template) and `clean_output` (preambles, quotes, empty output, runaway output).
- A `FakeRewriteEngine` for state-manager tests covering success, error, and changed focus.
- Memory-policy tests driven by fake timer events (load, stay warm, unload after idle).
- An `#[ignore]` integration test that runs the smallest registry model on a short input.
- A manual checklist covering Slack, Gmail, VS Code, Notes, a non-English text, and airplane mode.

## Acceptance criteria

- [ ] Rewrite replaces the selection in all four target apps.
- [ ] Rewrite works with networking turned off.
- [ ] The clipboard is unchanged after a rewrite that used the copy fallback.
- [ ] Engine failure leaves the original text intact and shows a notification.
- [ ] Non-English input stays in the same language.
- [ ] The model is unloaded after the idle period.

## Open questions

- Should an optional "dictate then rewrite" mode run rewrite over transcriptions automatically?
