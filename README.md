# jAyDHD-Grammar-Checker

A small personal writing helper for Windows, written in Rust.

## Demo

https://github.com/user-attachments/assets/eae5362c-8d86-49e9-8bb9-044048ec4950

It started out as a simple **word counter**, then more features were added just for fun.
It does have a grammar checker, but it's **not a robust one**. Think of this as a personal
side project, not a replacement for tools like Grammarly or Word's spell check.

## What it does

A small **control panel** turns features on and off. Each feature opens as its own little
floating box that you can drag anywhere on your screen. The boxes read the text you're typing
in Notepad, Word, web text boxes and Google Docs.

| Feature | What it does |
|---|---|
| **Word Count & Reading Level** | Counts words and estimates how easy the text is to read |
| **Keyword Density** | Shows which words you use the most |
| **Sentence Length** | Points out sentences that run long |
| **Grammar Checker** | Flags common mistakes like its/it's, their/there/they're, a/an and simple agreement errors |
| **Dialogue Formatting** | Checks the punctuation around quoted dialogue |
| **Thesaurus** | Shows synonyms and meanings for a word you select or type in |
| **Typing pets** | Little pets that react while you type (inspired by [Typing Pet](https://github.com/swoonqx/TypingPet)) |

**Extras:**
- A tray icon. Left-click it to show the panel, right-click it for the menu.
- An option to keep the boxes on top of other windows.
- A Test text box in the control panel, so you can try the features without another program.

## How to use it

### Option 1: Download the .exe
If there's a file on the **Releases** page, download `jAyDHD-Grammar-Checker.exe` and
double-click it. It's a single file with everything built in.

### Option 2: Build it yourself
1. Install Rust from https://rustup.rs
2. Open a terminal in this folder and run:
   ```
   cargo build --release
   ```
   The first build downloads everything and takes a while. A yellow `CRAFT_I18N_DIR`
   warning is normal.
3. The program is at `target\release\jAyDHD-Grammar-Checker.exe`. Copy it anywhere you
   like, for example `C:\Programs\jAyDHD\`.
4. Optional: run `cargo clean` afterwards to delete the large `target` folder.

Your settings are saved in `%APPDATA%\GrammarDesk\`, so moving or updating the .exe keeps them.

## Google Docs (optional)
The app can read and check a Google Doc open in Firefox. This needs your own Google API file
(`client_secret.json`) placed in `%APPDATA%\GrammarDesk\`. The control panel walks you
through signing in.

You can also click into any document and press **Ctrl+Alt+G** to check the whole thing, or
select text and press **Ctrl+Alt+S** to check only that part.

**Never share or upload** `client_secret.json` or `google_token.json`. They are your private
sign-in files.

## Credits
- The **Typing pets** feature is inspired by [Typing Pet](https://github.com/swoonqx/TypingPet)
  by swoonqx, a desktop pet that reacts as you type. The idea came from there; the code in this
  project was written separately and is not affiliated with Typing Pet.
- The Thesaurus uses **Princeton WordNet 3.1**, (c) Princeton University, under the
  [WordNet license](https://wordnet.princeton.edu/license-and-commercial-use).
- The readability check uses the **Dale-Chall** familiar word list.
