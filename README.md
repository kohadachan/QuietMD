# QuietMD

A tiny, portable, read-only Markdown viewer for Windows.

[Download the latest release](../../releases/latest)

The release executable is currently unsigned, so Windows SmartScreen may show a warning on first launch.

QuietMD is a single native executable built with Rust, Win32, Direct2D, and DirectWrite. It does not create settings files, history, caches, registry entries, or AppData folders, and it never connects to the network.

## Highlights

- Portable single-file application; no installer required
- Responsive word wrapping and per-monitor DPI support
- Mouse text selection, sentence selection by double-click, and clipboard copy
- Web search for selected text through the default browser
- Native Find dialog with forward and backward search
- Configurable font, text size, and line spacing for the current session
- Measured Markdown table columns with cell wrapping in narrow windows
- Monospaced `Consolas` rendering for code blocks and inline code
- Window layouts for the left or right third and half of the current monitor
- Automatic reload with scroll position preserved when the open file changes
- UTF-8, UTF-8 BOM, and UTF-16 LE/BE BOM support
- HTML is displayed as text rather than executed
- Remote images are never downloaded and links never open automatically
- Link destinations remain visible and copyable

## Requirements

- 64-bit Windows 10 or Windows 11

## Usage

Run `QuietMD.exe`, then drop a Markdown file onto the window. You can also drop a file onto the executable or associate `.md` files with QuietMD.

| Action | Input |
| --- | --- |
| Open a file | `Ctrl+O` |
| Select text | Mouse drag |
| Select one sentence | Double-click |
| Copy / Select all | `Ctrl+C` / `Ctrl+A` |
| Search selected text on the web | Right-click → `Search the web` |
| Find / Find next / Find previous | `Ctrl+F` / `F3` / `Shift+F3` |
| Clear selection | `Esc` |
| Open the context menu | Right-click or `Ctrl+,` |
| Reload | `F5` |
| Page down / up | `Space` / `Shift+Space` |
| Close / Quit | `Ctrl+W` / `Ctrl+Q` |
| Scroll | Mouse wheel, arrows, Page Up/Down, Home/End |
| Change text size | `Ctrl+mouse wheel` |

The context menu contains display settings and four window layouts: `Left third`, `Right third`, `Left half`, and `Right half`.

Files without a BOM must be valid UTF-8; unsupported encodings produce an error instead of replacement characters.

## Fonts

The default is Segoe UI at size 15. General-purpose choices are Segoe UI, Arial, Georgia, and Consolas. Japanese-oriented choices are BIZ UDGothic, Yu Gothic UI, and Meiryo. DirectWrite supplies system fallback glyphs when a selected family does not contain a character.

## Build from source

Install the Rust toolchain and Windows SDK, then run:

```powershell
.\build.ps1
```

The build cache stays inside `.cargo-home`, and the release executable is written to `dist\QuietMD.exe`.

## License

QuietMD is available under the [MIT License](LICENSE).

---

## 日本語

QuietMDは、Markdownを安全に読むことだけに特化した、超軽量・ポータブルなWindowsネイティブビューアです。

- インストール不要の単体EXE
- 設定ファイル、履歴、キャッシュ、レジストリ、AppData、通信、テレメトリなし
- ウィンドウ幅に追従する折り返しと、モニターごとのDPIに対応
- ドラッグ選択、ダブルクリックによるセンテンス全体の選択、コピーに対応
- 選択文字列を右クリックの「Search the web」から既定ブラウザでGoogle検索
- `Ctrl+F`、`F3`、`Shift+F3`による前後検索
- フォント、文字サイズ、行間をセッション中だけ変更可能
- `Ctrl+マウスホイール`で文字サイズを変更
- Markdown表は内容に合わせて列を整列し、狭い画面ではセル内で折り返し
- コードブロックとインラインコードは等幅フォント`Consolas`で表示
- 現在のモニターで左1/3、右1/3、左1/2、右1/2へ配置可能
- HTMLは実行せず文字として表示し、リンクも自動では開かない
- リモート画像は取得しない
- リンク先は表示・コピーできるが、自動では開かない
- ファイル更新時はスクロール位置を保ったまま自動再読込

UTF-8ではないBOMなしテキストは、文字化けさせずエラーとして通知します。ビルドにはRustツールチェーンとWindows SDKが必要です。`.\build.ps1`を実行すると、`dist\QuietMD.exe`が生成されます。
