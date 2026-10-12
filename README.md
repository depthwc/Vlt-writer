<p align="center"><img src="src-tauri/icons/128x128.png" width="96" alt=""></p>

<h1 align="center">Vlt-writer</h1>

<p align="center">Writes your documents in your own handwriting, on photos of your own paper.<br>For Windows 10 and 11 (64-bit). Everything stays on your computer.</p>

<p align="center"><a href="https://github.com/depthwc/Vlt-writer/releases/latest"><img src="https://img.shields.io/badge/Download-Windows-7c3aed?style=for-the-badge" alt="Download for Windows"></a></p>

## Use

1. **Handwriting**: write the sample pages and add their photos.
2. **Paper**: add a photo of the paper you write on.
3. **Write**: type or import a document, then export the pages as photos or PDF.

The installer is not signed, so Windows may warn: choose **More info → Run anyway**.

## Build

Needs Rust, Node 24 and pnpm.

```
pnpm install
pnpm --dir ui install
pnpm dev      # run the app
pnpm build    # installer in target/release/bundle/nsis
```

## License

[MIT](LICENSE) © depthwc. Free to use and change. If you publish it, or a changed version, keep the license and credit depthwc.
