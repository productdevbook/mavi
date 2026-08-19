import type { Extensions } from "@tiptap/core"
import { ReactNodeViewRenderer } from "@tiptap/react"
import StarterKit from "@tiptap/starter-kit"
import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight"
import {
  Details,
  DetailsContent,
  DetailsSummary,
} from "@tiptap/extension-details"
import { Emoji, gitHubEmojis } from "@tiptap/extension-emoji"
import FileHandler from "@tiptap/extension-file-handler"
import Highlight from "@tiptap/extension-highlight"
import Image from "@tiptap/extension-image"
import { TaskItem, TaskList } from "@tiptap/extension-list"
import Mention from "@tiptap/extension-mention"
import Subscript from "@tiptap/extension-subscript"
import Superscript from "@tiptap/extension-superscript"
import { TableKit } from "@tiptap/extension-table"
import {
  TableOfContents,
  getHierarchicalIndexes,
  type TableOfContentData,
} from "@tiptap/extension-table-of-contents"
import TextAlign from "@tiptap/extension-text-align"
import {
  BackgroundColor,
  Color,
  FontFamily,
  FontSize,
  LineHeight,
  TextStyle,
} from "@tiptap/extension-text-style"
import { Markdown } from "@tiptap/markdown"
import Heading from "@tiptap/extension-heading"
import Paragraph from "@tiptap/extension-paragraph"
import Underline from "@tiptap/extension-underline"
import Typography from "@tiptap/extension-typography"
import UniqueID from "@tiptap/extension-unique-id"
import Youtube from "@tiptap/extension-youtube"
import {
  CharacterCount,
  Focus,
  Placeholder,
  Selection,
} from "@tiptap/extensions"
import { t } from "@lingui/core/macro"
import { toast } from "sonner"

import { ApiRefused } from "@/lib/api"
import { upload } from "@/lib/upload"
import { publicFileUrl } from "@/lib/media"
import { apiMessage } from "@/lib/auth"
import { slugify } from "@/lib/editor-utils"
import {
  alignedHeadingMarkdown,
  alignedParagraphMarkdown,
  highlightMarkdown,
  subscriptMarkdown,
  superscriptMarkdown,
  textStyleMarkdown,
  underlineMarkdown,
} from "@/components/editor/extensions/comark"
import { CodeBlockView } from "@/components/editor/code-block-view"
import { lowlight } from "@/components/editor/extensions/languages"
import { SearchReplace } from "@/components/editor/extensions/search-replace"
import { SlashCommand } from "@/components/editor/extensions/slash-command"
import {
  emojiSuggestion,
  mentionSuggestion,
} from "@/components/editor/extensions/suggestions"

export interface BuildExtensionsOptions {
  characterLimit?: number | null
  onTocUpdate: (data: TableOfContentData) => void
  scrollParent: () => HTMLElement | Window
}

export function buildExtensions({
  characterLimit,
  onTocUpdate,
  scrollParent,
}: BuildExtensionsOptions): Extensions {
  return [
    StarterKit.configure({
      codeBlock: false,
      link: {
        openOnClick: false,
        autolink: true,
        defaultProtocol: "https",
        HTMLAttributes: { rel: "noopener noreferrer nofollow" },
      },
      horizontalRule: { HTMLAttributes: { class: "mavi-hr" } },
      undoRedo: { depth: 200 },
      // Replaced below so they can write themselves in a form comark reads.
      underline: false,
      paragraph: false,
      heading: false,
    }),

    Underline.extend({ renderMarkdown: underlineMarkdown }),
    Paragraph.extend({ renderMarkdown: alignedParagraphMarkdown }),
    Heading.extend({ renderMarkdown: alignedHeadingMarkdown }).configure({
      levels: [1, 2, 3, 4, 5, 6],
    }),

    CodeBlockLowlight.extend({
      addNodeView: () => ReactNodeViewRenderer(CodeBlockView),
    }).configure({ lowlight, defaultLanguage: "typescript" }),

    Markdown,

    // The individual marks rather than the kit, so the one carrying colour and
    // size can write itself as a span comark understands.
    TextStyle.extend({ renderMarkdown: textStyleMarkdown }),
    Color,
    BackgroundColor,
    FontFamily,
    FontSize,
    LineHeight,

    Highlight.extend({ renderMarkdown: highlightMarkdown }).configure({
      multicolor: true,
    }),
    Subscript.extend({ renderMarkdown: subscriptMarkdown }),
    Superscript.extend({ renderMarkdown: superscriptMarkdown }),
    Typography,

    TextAlign.configure({
      types: ["heading", "paragraph", "image"],
      alignments: ["left", "center", "right", "justify"],
    }),

    TaskList,
    TaskItem.configure({ nested: true }),

    TableKit.configure({
      table: { resizable: true, allowTableNodeSelection: true },
    }),

    Image.configure({
      inline: false,
      allowBase64: true,
      resize: { enabled: true, minWidth: 80, minHeight: 40 },
    }),

    Youtube.configure({
      controls: true,
      nocookie: true,
      modestBranding: true,
      width: 720,
      height: 405,
    }),

    Details.configure({
      persist: true,
      HTMLAttributes: { class: "mavi-details" },
    }),
    DetailsSummary,
    DetailsContent,

    Emoji.configure({
      emojis: gitHubEmojis,
      enableEmoticons: true,
      suggestion: emojiSuggestion,
    }),

    Mention.configure({
      HTMLAttributes: { class: "mavi-mention" },
      suggestion: mentionSuggestion,
    }),

    FileHandler.configure({
      // Mirrors the backend's sniffed allow-list; SVG is excluded there
      // because it can carry script and uploads are served same-origin.
      allowedMimeTypes: ["image/png", "image/jpeg", "image/gif", "image/webp"],
      onDrop: (editor, files, pos) => {
        files.forEach(async (file) => {
          try {
            const media = await upload(file, "public")
            editor
              .chain()
              .insertContentAt(pos, {
                type: "image",
                attrs: { src: publicFileUrl(media.id), alt: file.name },
              })
              .focus()
              .run()
          } catch (error) {
            toast.error(
              error instanceof ApiRefused
                ? apiMessage(error)
                : t`Could not upload ${file.name}`
            )
          }
        })
      },
      onPaste: (editor, files) => {
        files.forEach(async (file) => {
          try {
            const media = await upload(file, "public")
            editor
              .chain()
              .insertContentAt(editor.state.selection.anchor, {
                type: "image",
                attrs: { src: publicFileUrl(media.id), alt: file.name },
              })
              .focus()
              .run()
          } catch (error) {
            toast.error(
              error instanceof ApiRefused
                ? apiMessage(error)
                : t`Could not upload ${file.name}`
            )
          }
        })
      },
    }),

    UniqueID.configure({
      types: [
        "heading",
        "paragraph",
        "blockquote",
        "codeBlock",
        "image",
        "table",
      ],
    }),

    TableOfContents.configure({
      getIndex: getHierarchicalIndexes,
      getId: (text) => slugify(text) || t`section`,
      scrollParent,
      onUpdate: (data) => onTocUpdate(data),
    }),

    Placeholder.configure({
      includeChildren: true,
      placeholder: ({ node, editor, pos }) => {
        if (node.type.name === "heading") {
          return t`Heading ${node.attrs.level}`
        }
        if (node.type.name === "detailsSummary") {
          return t`Section title`
        }
        if (node.type.name === "codeBlock") {
          return ""
        }
        if (pos === 0 && editor.isEmpty) {
          return t`Start writing, or type “/” for commands…`
        }
        return t`Keep going, type “/” to add a block…`
      },
    }),

    Focus.configure({ className: "mavi-focused", mode: "shallowest" }),
    Selection,

    CharacterCount.configure({
      limit: characterLimit ?? undefined,
      mode: "textSize",
    }),

    SlashCommand,
    SearchReplace,
  ]
}
