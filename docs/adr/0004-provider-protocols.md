# ADR-0004: Provider protocol adapters and tool-call verification

- **Status:** Accepted (M2)
- **Scope:** S-020, S-021, S-022, S-023. Required before S-020 by the backlog.

## Context

The agent needs a model that makes genuine structured tool calls. The spec
forbids executing commands parsed from assistant text, forbids claiming a
provider works from a plain text reply, and forbids sending secrets to a
redirect target. Three wire protocols are in scope: OpenAI Responses, OpenAI
Chat Completions (also used by many compatible servers), and Anthropic Messages.

## Decisions

1. **One adapter per wire protocol, no trait objects.** `selai-llm` dispatches
   with a `match` on `ProviderProtocol`. The provider list is closed and each
   adapter is small, so dynamic dispatch buys nothing and complicates `Send`
   futures. A future protocol needs a new variant and a new module.

2. **Normalized events.** Adapters emit `AgentEvent` values: `TextDelta`,
   `ToolCallStart`, `ToolCallArgumentsDelta`, `ToolCallReady`, `TokenUsage`,
   `Completed`, `Error`. Events go through a bounded `tokio::sync::mpsc` channel,
   so a slow consumer applies backpressure to the HTTP stream.

3. **A tool call exists only when its arguments are complete.** Argument
   fragments are buffered per call. `ToolCallReady` is emitted only after the
   buffer parses as a JSON object. Invalid JSON raises `PROVIDER_PROTOCOL_ERROR`
   and ends the turn. Nothing is executed from a partial call.

4. **A turn must end cleanly.** Each protocol has a terminal marker: `[DONE]` or
   stream close with a finish reason for Chat, `response.completed` for
   Responses, `message_stop` for Anthropic. A stream that ends without its
   marker is an error, so a truncated response cannot pass as a complete one.

5. **Retries are narrow.** A request is retried only when the server answers
   `429`, `503` or `504` before any body arrives, up to two times with a fixed
   backoff. Nothing is retried after output has started, and nothing is retried
   after a tool call was returned. Tools are never replayed on a network retry.

6. **Secrets stay in the adapter.** The API key is read from `SecretValue` only
   to build the auth header (`Authorization: Bearer` or `x-api-key`). It is not
   written to the body, logs or errors. Provider error bodies are truncated to
   300 characters and shown only after a sanitising pass. Tests assert that the
   key appears in the header and nowhere in the request body.

7. **No redirects.** The HTTP client does not follow redirects. A `3xx` is an
   error, so an API key can never reach a different origin. Plain `http://` is
   allowed only for loopback hosts (`localhost`, `127.0.0.0/8`, `::1`), for local
   Ollama, vLLM and LiteLLM. Userinfo, query and fragment in base URLs are
   rejected.

8. **No provider fallback.** A failed request is reported. Switching to another
   provider mid-task requires explicit opt-in (not in M2).

9. **Verification is an explicit probe, not a status code.**
   - `Test connectivity`: one small request without tools. Success means a
     non-empty reply.
   - `Test tool calling`: sends a benign `selai_echo` tool, requires a real tool
     call with a non-empty `text` argument, returns a fixed tool result, and
     requires a final text reply. Only then is the provider marked
     `tool_probe_passed`. The probe executes no SSH and no local command.
   - The stored result is tied to `protocol | base_url | model_id`. Changing any
     of these clears the result.

10. **Templates are suggestions.** The template list gives a default base URL
    and a protocol. It never gives a model ID, because model names change.
    Templates are marked "not verified against a live endpoint" until a
    passing probe exists. For GLM and Kimi, the URLs were taken from third-party
    integration docs on 2026-10-16 and must be confirmed against the vendor's
    own documentation.

## Consequences

- The adapters can be tested without credentials, using mock HTTP servers that
  emit real SSE shapes, including fragmented arguments and invalid JSON.
- Live provider validation is a separate step that needs user-supplied keys.
  Until it runs, every provider is shown as "not verified against a live
  endpoint". No other claim is made.
- Anthropic and OpenAI wire formats are implemented from their public
  documentation. Field names that could not be checked against a live server are
  listed in the M2 report.

## Fallback

If a protocol needs features these adapters do not handle (for example,
reasoning-state round-trips in Responses), the provider is marked
`PROVIDER_TOOL_UNSUPPORTED` for tool calling, not partially supported.
