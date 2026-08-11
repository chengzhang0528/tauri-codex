# Third-Party Notices

tauri-codex includes and depends on third-party software. The GPL license for project-owned code does not replace, restrict, or relicense these components. Versions used by a particular build are fixed by `app/build-versions.json`, `app/package-lock.json`, and `app/src-tauri/Cargo.lock`.

## Bundled runtime components

| Component | Version | License | Source |
|---|---:|---|---|
| OpenAI Codex CLI (`@openai/codex` and Windows x64 package) | 0.147.0 | Apache-2.0 | https://github.com/openai/codex |
| Node.js Windows x64 installer | 24.19.0 | MIT and bundled third-party notices | https://github.com/nodejs/node/tree/v24.19.0 |

The Node.js installer is redistributed as an unmodified official MSI. Its complete composite license and bundled dependency notices are maintained by Node.js at https://github.com/nodejs/node/blob/v24.19.0/LICENSE.

OpenAI and Codex are trademarks or product names of their respective owner. tauri-codex is an independent community project and is not an official OpenAI product.

## Frontend components

| Component | License | Source |
|---|---|---|
| Tauri JavaScript API and dialog/opener plugins | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri and https://github.com/tauri-apps/plugins-workspace |
| xterm.js and addon-fit | MIT | https://github.com/xtermjs/xterm.js |
| Lucide icons | ISC | https://github.com/lucide-icons/lucide |

## Rust components

The application directly uses Tauri, Tauri dialog/opener plugins, serde, serde_json, portable-pty, uuid, toml, toml_edit, reqwest, sha2, semver, tiny_http, and windows-rs. These projects are distributed under their upstream MIT, Apache-2.0, or dual MIT/Apache-2.0 terms. Exact versions and transitive packages are recorded in `app/src-tauri/Cargo.lock`.

## License texts

### Apache License 2.0

The complete Apache License 2.0 text distributed with this project is available at [LICENSES/Apache-2.0.txt](LICENSES/Apache-2.0.txt).

### MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy of MIT-licensed software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The applicable upstream copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

### ISC License

Permission to use, copy, modify, and/or distribute ISC-licensed software for any purpose with or without fee is hereby granted, provided that the applicable upstream copyright notice and this permission notice appear in all copies.

ISC-LICENSED SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

This notice is a project-maintained summary. Upstream package license files and notices remain authoritative for third-party components.
