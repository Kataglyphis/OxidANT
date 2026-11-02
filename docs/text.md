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

**The key never reaches a log, and a server cannot grow memory without bound.** `Endpoint`'s
`Debug` prints `api_key` as `Some("<redacted>")`; its implementation destructures the struct, so a
new field fails to compile until it is printed or redacted on purpose. The streamed reader
(`endpoint::read_stream_line`) reads at most `STREAM_LINE_LIMIT` (1 MiB) per line, its `\n` or
`\r\n` aside, and refuses a longer one with `TextError::StreamLineTooLong(limit)` instead of
buffering it. A chunk's JSON sits on one `data:` line, so the bound limits a single piece of text,
not the reply.

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

`GenerationOptions` also carries three optional controls, on both backends:

| Option | GenAI | Endpoint |
| --- | --- | --- |
| `seed` | the `random_seed` search option; one seed, model and provider repeat their tokens | `seed` |
| `json_schema` | `OgaGeneratorParamsSetGuidance("json_schema", ...)`, which needs a GenAI built with llguidance | `response_format` with `type: json_schema` and `strict: true` |
| `stop` | the decode loop ends before the first stop string, which the text leaves out; `finish_reason` is `Stop` | `stop` |

`genai::configure` decides every search option and the guidance through the `SearchParams` trait,
so the unit tests record what GenAI would receive without loading it. Text that may begin a stop
string is held back from `on_text` until it is decided. The Linux chain builds GenAI with
llguidance; the Windows chain does not, and its generator refuses a schema with
`TextError::JsonSchemaUnsupported` rather than answering unconstrained.

Each failure has its own `TextError`: the library, the model directory, the provider, the
tokenizer, the chat template, the encoding, invalid options (no messages, a zero budget, a NUL
byte, a seed above `i32::MAX`, an empty stop string, a `json_schema` that is not a JSON object),
generation, an unreachable endpoint, an endpoint HTTP status with its body, a streamed line over
the 1 MiB bound, and a `json_schema` this lane cannot hold.

Not yet: embeddings from GenAI. `embed` needs an endpoint.

## The CLI

```bash
kataglyphis_cli chat --model-dir <genai-model-dir> --prompt "What is the capital of France?"
kataglyphis_cli chat --backend ollama --model qwen3:4b --prompt "Hallo" --system "Answer in German."
kataglyphis_cli embed --base-url http://localhost:11434 --model nomic-embed-text "first" "second"
```

`chat` streams the reply to stdout and prints the token count, the time to the first token, the
decode rate and the finish reason to stderr. `--seed N`, `--json-schema '<schema JSON>'` and a
repeatable `--stop TEXT` set the three options above. `--provider cuda` or `dml` selects GenAI's execution
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

`TextOptions` carries `seed`, `json_schema` and `stop` as above. A schema on a lane without
llguidance fails `text_generate` with a message starting `json_schema is unsupported on this lane`.
`TextSource` names either a `model_dir` with a `provider` (`cpu`, `cuda`, `dml`) or the endpoint
fields, plus an `api_key` for platforms without the key's variable. One generation runs at a time,
and the last local model stays loaded for the next turn. Android and macOS reach endpoints only;
the web build reaches language models from Dart, not through this bridge.

## Tests

Only the contract suite loads a released model. The GenAI suites write
[the hub's tiny GenAI model](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/language-ai.md#the-test-model)
with its generator, `third_party/ANTfrastructure/linux/scripts/lib/tiny-genai-model.py`, and read
its `fixture.json` oracle. Greedy decoding of `abc` yields `defg`, and from the byte `0xBF` the
model runs to its end token. Where no GenAI loads they print `SKIP`; CI sets
`KATAGLYPHIS_REQUIRE_GENAI=1`, which turns that into a failure.

`crates/text/tests/contract.rs` replays ModelANT's `text-generation-v1` contract, a byte-identical
copy in `crates/text/tests/contracts/`, on SmolLM2-135M-Instruct: greedy decoding of
capital-of-france must yield exactly `[504, 3575, 282, 4649, 314, 7042, 30, 2]` and contain
`Paris`. `resources/models/llm.lock` pins the archive with OrchestrANT's row;
`scripts/linux/fetch-models.sh --llm` runs the hub's `fetch-data-lock.py --lock
resources/models/llm.lock --extract` in the Linux test and coverage steps, and with
`KATAGLYPHIS_REQUIRE_MODEL=1` a missing model fails. The riscv64 lane runs this suite on scalar
MLAS, because ONNX Runtime's RVV path crashes on this model
([the riscv64 lane](ci.md#the-riscv64-lane)). Windows CI unpacks it into
`target\llm-models` and names that through `KATAGLYPHIS_LLM_MODELS`, because `Build-Windows.ps1`
bundles `resources\`. No installer carries the model. The endpoint suites run against a
local server that plays back one recorded answer and ends within 30 s on its own.

| Suite | What it covers |
| --- | --- |
| `crates/text/src/*.rs` | Option checks, the request body, stream lines and their 1 MiB bound, the redacted key, embeddings, endpoint resolution, the loader's search and its chain check |
| `crates/text/tests/genai.rs` | The tiny model: the oracle's tokens and text, the end token, a cancel from another thread, invalid requests, a repeated seed's identical ids, at least two different 24-token answers across seeds 1 to 4 at temperature 1, a schema answer that parses and validates on Linux and `JsonSchemaUnsupported` on Windows, a stop string ending with `Stop` |
| `crates/text/tests/contract.rs` | ModelANT's text-generation-v1 on SmolLM2-135M: exact ids and `must_contain` |
| `crates/text/tests/endpoint.rs` | Streaming, cancel, both finish reasons, HTTP errors, an over-long stream line, an unreachable port, embeddings, `seed`, `stop` and `response_format` in the request body |
| `crates/cli/tests/text.rs` | `chat` and `embed` on both backends, a named backend, the missing-endpoint error, the `--seed`, `--json-schema` and `--stop` request body |
| `src/api/text.rs` | The bridge's mapping |

Five mutation recipes in `scripts/linux/tests/ort-license-mutations.json` run on the hosted
mutation lane without GenAI: dropping the seed or the guidance in `genai::configure`, switching
greedy decoding to sampling, and dropping the endpoint's `seed` or `response_format`. They do not
cover the FFI calls that apply `configure`'s choices, which only the GenAI suites reach.
