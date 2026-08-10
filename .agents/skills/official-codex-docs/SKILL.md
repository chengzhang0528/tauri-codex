---
name: official-codex-docs
description: Use when official OpenAI Codex documentation must be fetched on this Windows workspace, especially when direct developers.openai.com requests return 403.
---

# Official Codex Docs

Use only for read-only documentation retrieval. It does not configure the tauri-codex application, Codex runtime, Server profiles, or desktop updater.

## Source and URL

- Keep the requested `developers.openai.com` URL as the source of truth.
- When the page supports Markdown, append `.md` to the exact page URL. Follow redirects; the current official pages may redirect to `learn.chatgpt.com`.
- Do not substitute third-party mirrors, search snippets, or copied examples for the official page.

## Windows fetch order

Use the bundled PowerShell helper first when direct access is known to be blocked:

```powershell
$codexDocsUrl = 'https' + '://developers.openai.com/codex/cli/reference.md'
& .agents/skills/official-codex-docs/scripts/fetch-official-codex-doc.ps1 `
  -Url $codexDocsUrl
```

The helper uses `CODEX_DOCS_PROXY` when set. Otherwise it tries the local proxy at
`localhost:1080`, then the IPv4 equivalent `127.0.0.1:1080`, and only then tries
direct HTTPS. This proxy is a documentation-fetch aid, never an application
runtime or updater prerequisite. The helper writes no cache unless `-OutputPath`
is explicitly supplied.

For the broad Codex manual, the official OpenAI helper remains the fallback. In PowerShell, expose the local proxy only for that command and use a temporary cache:

```powershell
$localProxy = 'http' + '://localhost:1080'
$env:HTTPS_PROXY = $localProxy
$env:HTTP_PROXY = $env:HTTPS_PROXY
node "$env:USERPROFILE\.codex\skills\.system\openai-docs\scripts\fetch-codex-manual.mjs" `
  --cache-dir "$env:TEMP\openai-docs-cache"
```

If the local proxy is absent or fails, unset those variables and use the official helper or OpenAI Docs MCP. Never turn proxy availability into a product prerequisite.

## Evidence

The verified current path is:

```text
direct developers.openai.com/codex/cli/reference.md -> HTTP 403
local proxy localhost:1080 -> HTTP 200
```

The request must remain read-only, use an explicit timeout, and never persist tokens, proxy credentials, response logs, or customer data.
