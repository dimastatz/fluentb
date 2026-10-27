# M5 — Rewrite

**Status:** Done

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
4. **Memory policy** — load the model on the first rewrite, keep it warm, and unload it after a configurable idle period (default 1 minute) so idle RAM stays low. `rewrite.keep_loaded = true` disables unloading.
5. **Prompt** — built by a pure function `fn build_prompt(text, style) -> Prompt` using the model's chat template, with these rules:
   - Fix grammar, spelling, and punctuation; improve flow.
   - Keep meaning, facts, names, URLs, code spans, emoji, and the language.
   - Apply the configured style (neutral, casual, formal, concise, technical). `technical` was added later (issue #22): structured and precise for engineers and architects, with its own worked examples.
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

- [x] Rewrite replaces the selection in all four target apps.
- [x] Rewrite works with networking turned off.
- [x] The clipboard is unchanged after a rewrite that used the copy fallback.
- [x] Engine failure leaves the original text intact and shows a notification.
- [x] Non-English input stays in the same language.
- [x] The model is unloaded after the idle period.

## Implementation notes

Decisions made while building M5:

- **Core:** the rewrite flow from M1 gained one step. After a rewrite, the core asks the facade to `replace_selection(original, replacement)`. The facade re-reads the selection and pastes only if it still holds the original text; otherwise it copies the result to the clipboard (`Replacement::Copied`), and the core notifies the user. No `RewriteEngine` trait was needed: `FakeFacade` covers the state-manager tests, and llama.cpp runs any GGUF model, so swapping models is a config change.
- **Pure helpers** in `src/rewrite/`, all fully tested:
  - `prompt` builds ChatML (Qwen3, with thinking switched off by an empty think block) and Gemma prompts as segments. Template parts are tokenized with special tokens and the user's text without them, so a selection containing `<|im_end|>` can't break out of its turn.
  - `output::clean_output` strips think blocks, `<text>` tags, preambles, wrappers and line-end spaces, rejects empty or runaway output (over 3× the input plus 20 characters), and keeps the original's surrounding whitespace.
  - `check_length` enforces the selection limit, and `max_new_tokens` caps generation at 1.5× the input plus 64 tokens.
  - `residency::UnloadPolicy` decides when to unload the model.
- **Reading the selection (macOS):**
  - The Accessibility API is tried first (`AXFocusedUIElement` → `AXSelectedText`). Before reading, fluentb sets `AXManualAccessibility` on the focused app, so Electron apps (Slack, VS Code) expose their text.
  - If that returns nothing, fluentb presses Cmd+C and polls the clipboard for up to 500 ms, then restores it.
- **Engine:** `llama-cpp-2` with Metal, loaded on the first rewrite, with every layer on the GPU, temperature 0.2 and a fixed seed. llama.cpp and ggml logging is switched off, so no text is ever printed.
  - The worker wakes up when the idle period ends (`bridge::next_route_within`) and drops the model to free its memory.
- **Linking:** whisper-rs links its own ggml statically, so a second static ggml from llama.cpp produced hundreds of duplicate-symbol warnings: one copy would silently replace the other. llama.cpp is therefore linked dynamically (`dynamic-link`). `build.rs` adds rpaths for `cargo run`, test binaries and the app bundle's `Frameworks` folder (M8 must copy the dylibs there).
- **Model choice:** see [`eval/rewrite/RESULTS.md`](../../eval/rewrite/RESULTS.md). Qwen3-4B and Gemma 3 4B were close on 16 samples, so the licence rule made Qwen3-4B the default.
  - Measured: about 1.5 s to load and 2–2.5 s for a typical message.
  - The light Qwen3-1.7B takes about 1 s.
- **Privacy:** `cargo tree -p llama-cpp-2` contains no HTTP or networking crates.
- **Config:** `[rewrite]` gained `model`, `keep_loaded`, `unload_after_minutes` and `max_characters`.

## Manual checklist

- [x] Rewrite replaces the selection in Slack, Gmail (Chrome or Safari), VS Code and Notes.
- [x] Hebrew or another non-English text stays in its language.
- [x] Works with Wi-Fi turned off.
- [x] Changing the selection during a rewrite copies the result to the clipboard and notifies.
- [x] The clipboard holds its previous text after a rewrite that used the Cmd+C fallback.
- [x] Rewriting with no selection notifies "Select some text to rewrite". (VS Code copies the whole line when nothing is selected; check this case.)
- [x] With `unload_after_minutes = 1`, memory drops about a minute after a rewrite.

## Open questions

- Should an optional "dictate then rewrite" mode run rewrite over transcriptions automatically?
