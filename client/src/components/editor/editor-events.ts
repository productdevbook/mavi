export type EditorDialogEvent =
  | "image-upload"
  | "image-url"
  | "youtube"
  | "link"
  | "table"
  | "find-replace"
  | "shortcuts"
  | "export"

const CHANNEL = "mavi-editor-dialog"

export function openEditorDialog(name: EditorDialogEvent) {
  window.dispatchEvent(new CustomEvent(CHANNEL, { detail: name }))
}

export function onEditorDialog(handler: (name: EditorDialogEvent) => void) {
  const listener = (event: Event) =>
    handler((event as CustomEvent<EditorDialogEvent>).detail)
  window.addEventListener(CHANNEL, listener)
  return () => window.removeEventListener(CHANNEL, listener)
}
