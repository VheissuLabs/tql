Press `a` and ask in plain english. The answer lands **in the editor**, with the
explanation as `--` comments above it, and nothing runs until you press `ctrl+r`.

```
-- Counts how many invoices each customer has. The join matches each invoice to
-- its customer on customer_id, group by makes count() run per customer, and
-- order by puts the busiest first.
--
select c.name, count(i.id) as invoices
from customers c
join invoices i on i.customer_id = c.id
group by c.id, c.name
order by invoices desc
limit 50
```

The model writes queries; it never runs them. It is asked for exactly one
statement, reads only, and is told to use nothing outside the schema, and it
still lands in front of you for review rather than in front of your database.

**Only table and column names are sent.** No row data ever leaves the machine, so
asking about a production table does not send its contents anywhere. The table
you are looking at is sent first so it survives the size limit.

## Asking

Press `a` and a modal opens with a text area. `↵` asks, `⇧↵` starts a new line,
`esc` cancels. `ctrl+s` sends it too. tql keeps drawing while the model thinks,
and `esc` while it is asking throws the answer away when it comes.

Terminals send the same byte for enter and shift+enter, so shift+enter only
arrives as its own key when the terminal is told to send one. In Ghostty:

```
keybind = shift+enter=csi:13;2u
```

Alt+enter works without any configuration, if you would rather not set that.

## Which model answers

Whatever you have a key for. With `provider = "auto"`, tql uses `url` if you set
one, and otherwise the first provider it finds a key for in your environment, so
setting `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is all it takes:

```toml
[ai]
provider = "auto"   # or anthropic, openai, azure, gemini, groq, mistral, deepseek, xai, openrouter, ollama
model = ""          # empty uses a sensible default for that provider
timeout = 60
```

| Provider | Key | Default model |
| --- | --- | --- |
| `anthropic` | `ANTHROPIC_API_KEY` | `claude-sonnet-5` |
| `openai` | `OPENAI_API_KEY` | `gpt-5` |
| `azure` | `AZURE_OPENAI_API_KEY`, with `AZURE_OPENAI_URL` | `gpt-4o` |
| `gemini` | `GEMINI_API_KEY` | `gemini-2.5-pro` |
| `groq` | `GROQ_API_KEY` | `llama-3.3-70b-versatile` |
| `mistral` | `MISTRAL_API_KEY` | `mistral-large-latest` |
| `deepseek` | `DEEPSEEK_API_KEY` | `deepseek-chat` |
| `xai` | `XAI_API_KEY` | `grok-4` |
| `openrouter` | `OPENROUTER_API_KEY` | `anthropic/claude-sonnet-5` |
| `ollama` | none, once `OLLAMA_URL` is set | `llama3.2` |

`auto` tries them in alphabetical order and takes the first that is set up, so
with several keys in your environment, name the one you want in `provider`. A
named provider with no key falls back to `url`. Each provider's address can be
moved with its own variable, such as `ANTHROPIC_URL` or `OPENAI_URL`, for a proxy
or a gateway. `TQL_AI_PROVIDER`, `TQL_AI_MODEL`, `TQL_AI_TIMEOUT`, `TQL_AI_URL`
and `TQL_AI_KEY` in the environment win over `config.toml`.

## A local model, with LM Studio

Anything with an OpenAI-compatible API works. LM Studio, start to finish:

1. Install a model in LM Studio. A coding model is the right shape for this, and
   `qwen2.5-coder-7b-instruct` is a good starting point.
2. Open the **Developer** tab and **Start Server**. It listens on port 1234 by
   default and speaks OpenAI's API at `http://localhost:1234/v1`.
3. Ask it what it is serving, and use that name verbatim:

   ```bash
   curl -s http://localhost:1234/v1/models | jq -r '.data[].id'
   # qwen2.5-coder-7b-instruct
   ```

4. Put both in `~/.config/tql/config.toml`:

   ```toml
   [ai]
   url = "http://localhost:1234/v1"
   model = "qwen2.5-coder-7b-instruct"
   key = ""            # LM Studio does not want one; some gateways do
   ```

5. Press `a` in tql and ask for something.

With `provider = "auto"`, a `url` wins over any key in your environment, since it
points somewhere deliberate. Nothing leaves your machine in that setup, and only
table and column names were ever being sent anyway.

Ollama is the same shape, on its own port:

```toml
[ai]
url = "http://localhost:11434/v1"
model = "qwen2.5-coder"
```

LM Studio may want a token. It says so in the Developer tab, and it goes in
`key`. A url with no path gets `/v1` added, so `http://localhost:1234` and
`http://localhost:1234/v1` both work.

A reasoning model such as qwen3 answers in its thinking rather than in the
message body, and tql reads the answer out of it either way.

## When it does not work

If `a` says there is no model to ask, tql could not find a url or a key, and it
says what to set. If it fails at the network, the server is not running or the
port is wrong, and the `curl` above is the quickest way to tell which.

The `[ai]` section in [Configuration](Configuration#ai) has every key and how
`TQL_AI_*` overrides them.
