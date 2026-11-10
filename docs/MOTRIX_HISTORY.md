# Recover Motrix Next download history on macOS

Rayburst uses a separate application data directory. To find the original history in
Finder, choose **Go → Go to Folder** and enter:

```text
~/Library/Application Support/com.motrix.next/
```

The old records are in `history.db`. Rayburst keeps its history in
`~/Library/Application Support/dev.aninsomniacy.rayburst/history.db`.

1. Open Rayburst once to initialize its database, then quit both applications.
2. [Download the recovery helper](https://raw.githubusercontent.com/AnInsomniacy/rayburst/main/scripts/recover_motrix_history.py).
   Open Terminal in the folder where you saved it and use Python 3 to preview how
   many records are missing from Rayburst:

   ```sh
   python3 recover_motrix_history.py
   ```

3. Import those records and reopen Rayburst:

   ```sh
   python3 recover_motrix_history.py --apply
   ```

The helper keeps the Motrix Next records in place and backs up the Rayburst database
before writing. It keeps Rayburst's record when both databases contain the same task
ID. Imported records appear in Rayburst's history. Settings, queued tasks, partial
download state and downloaded files stay in their original locations.
