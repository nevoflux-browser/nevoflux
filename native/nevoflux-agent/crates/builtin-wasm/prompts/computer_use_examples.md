# Computer Use Examples

## Example 1: Type and Save in a Text Editor

```
1. computer_screenshot()                          → See the editor window
2. computer_click(x=400, y=300)                   → Click in the editing area
3. computer_type_text(text="Hello, World!")        → Type the content
4. computer_key(key="s", modifiers=["ctrl"])       → Save with Ctrl+S
5. computer_wait(ms=500)                           → Wait for save dialog
6. computer_screenshot()                           → Verify save completed
```

## Example 2: Drag a File to a Folder

```
1. computer_screenshot()                          → Locate the file and target folder
2. computer_drag(start_x=200, start_y=300,
                 end_x=600, end_y=300)            → Drag file to folder
3. computer_wait(ms=1000)                         → Wait for move operation
4. computer_screenshot()                          → Verify file is in new location
```

## Example 3: Handle a Native File Upload Dialog

```
1. browser_click(selector="#upload-btn")          → Click upload button (triggers native dialog)
2. computer_wait(ms=1000)                         → Wait for OS file picker to open
3. computer_screenshot()                          → See the file picker dialog
4. computer_type_text(text="/home/user/photo.jpg") → Type the file path
5. computer_key(key="Enter")                      → Confirm selection
6. computer_wait(ms=500)                          → Wait for dialog to close
7. computer_screenshot()                          → Verify file was selected
```
