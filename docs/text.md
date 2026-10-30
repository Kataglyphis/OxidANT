# Language models

`crates/text` (`kataglyphis_text`) generates text through two backends that follow the family's
text contract,
[ANTfrastructure's `docs/language-ai.md` § The text contract](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/language-ai.md#the-text-contract):
messages in, decoded text out as it is generated, a cancel from any thread, and speed figures.
The CLI and the flutter_rust_bridge surface use it; OmniAccelerANT's AI Playground calls the latter.

## The two backends

- **`genai::GenaiGenerator`** runs a GenAI model directory in process on the chain's
  ONNX Runtime GenAI, on Linux and Windows. It loads `libonnxruntime-genai.so` or
  `onnxruntime-genai.dll` at run time, so nothing links it. The library is looked for beside the
  executable, then under `ONNX_GENAI_ROOT` (`bin`, then `lib`), then at the Linux image's
  `/usr/local/lib/onnxruntime-genai/lib`. There is no bare-name fallback, which would take
  whatever copy the loader finds first.
- **`endpoint::Endpoint`** talks to an OpenAI-compatible server: `/v1/chat/completions` with a
  server-sent stream, and `/v1/embeddings`. `EndpointChoice` resolves it in the hub tools' order:
  a URL, `LLM_BASE_URL` or `OLLAMA_BASE_URL`, a backend named in
  [`backends.json`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/linux/llm-stack/README.md#named-backends),
  then its `default` entry. The registry is the given path or `LLM_BACKENDS`. A backend's
  `api_key_env` names the variable that holds its key; its `request_extra` keys are sent first, so
  the caller's own win.

**Only the chain's GenAI loads.** As for ONNX Runtime, the loader reads the library and refuses
it unless it embeds the chain's build tree (`/opt/onnxruntime-genai/build/` on Linux,
`C:\temp\onnx-genai-src\build\` on Windows), which its bundled dependencies carry through
`__FILE__`. A pip, NuGet or downloaded copy fails with `LibraryUnavailable`.

## Generating

`TextBackend::generate(messages, options, cancel, on_text)` applies the model's own chat template,
then decodes at most `max_new_tokens` tokens. Temperature 0, the default, decodes greedily; the
endpoint then sends no `top_p` or `top_k`. `on_text` receives each piece on the generating thread.
In GenAI a token that ends inside a UTF-8 character yields no piece until the character completes.

The result holds the text, the token ids (GenAI only), the prompt and completion token counts,
the time to the first token and the decode rate after it. `finish_reason` is `Stop` at an end
token or the server's `stop`, `Length` when the budget ran out, and `Cancelled` after
`CancelToken::cancel`. A cancel ends the run after its current token or stream line, and each run
clears the token as it starts. A stream that ends without a finish reason is an error, not an
answer. A server without usage reports leaves the completion count at the number of pieces.

Each failure has its own `TextError`: the library, the model directory, the provider, the
tokenizer, the chat template, the encoding, invalid options (no messages, a zero budget, a NUL
byte), generation, an unreachable endpoint, and an endpoint HTTP status with its body.

Not yet: `seed`, a JSON schema for constrained output, and embeddings from GenAI. `embed` needs an
endpoint.

## The CLI

```bash
kataglyphis_cli chat --model-dir <genai-model-dir> --prompt "What is the capital of France?"
kataglyphis_cli chat --backend ollama --model qwen3:4b --prompt "Hallo" --system "Answer in German."
kataglyphis_cli embed --base-url http://localhost:11434 --model nomic-embed-text "first" "second"
```

`chat` streams the reply to stdout and prints the token count, the time to the first token, the
decode rate and the finish reason to stderr. `--provider cuda` or `dml` selects GenAI's execution
provider. `embed` prints one L2-normalised vector per text, one JSON array per line.

## The Flutter bridge

`src/api/text.rs` flattens the same calls for Dart:

| Function | What it does |
| --- | --- |
| `text_generate(source, messages, options, sink)` | Streams `TextEvent`s: `piece` with its text, then one `done` with the finish reason and the figures |
| `text_cancel()` | Cancels the running generation |
| `text_embed(source, texts)` | The endpoint's vectors |
| `text_local_available()` | Whether this build loads the chain's GenAI |
| `text_default_options()` | The contract's defaults |

`TextSource` names either a `model_dir` with a `provider` (`cpu`, `cuda`, `dml`) or the endpoint
fields, plus an `api_key` for platforms without the key's variable. One generation runs at a time,
and the last local model stays loaded for the next turn. Android and macOS reach endpoints only;
the web build reaches language models from Dart, not through this bridge.

## Tests

No suite loads a real model. The GenAI suites write
[the hub's tiny GenAI model](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/language-ai.md#the-test-model)
with its generator, `third_party/ANTfrastructure/linux/scripts/lib/tiny-genai-model.py`, and read
its `fixture.json` oracle. Greedy decoding of `abc` yields `defg`, and from the byte `0xBF` the
model runs to its end token. Where no GenAI loads they print `SKIP`; CI sets
`KATAGLYPHIS_REQUIRE_GENAI=1`, which turns that into a failure. The endpoint suites run against a
local server that plays back one recorded answer and ends within 30 s on its own.

| Suite | What it covers |
| --- | --- |
| `crates/text/src/*.rs` | Option checks, the request body, stream lines, embeddings, endpoint resolution, the loader's search and its chain check |
| `crates/text/tests/genai.rs` | The tiny model: the oracle's tokens and text, the end token, a cancel from another thread, invalid requests |
| `crates/text/tests/endpoint.rs` | Streaming, cancel, both finish reasons, HTTP errors, an unreachable port, embeddings |
| `crates/cli/tests/text.rs` | `chat` and `embed` on both backends, a named backend, the missing-endpoint error |
| `src/api/text.rs` | The bridge's mapping |
