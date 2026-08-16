import { Extension, type Range } from "@tiptap/core"
import type { Node as PMNode } from "@tiptap/pm/model"
import { Plugin, PluginKey, TextSelection } from "@tiptap/pm/state"
import { Decoration, DecorationSet } from "@tiptap/pm/view"

export interface SearchReplaceStorage {
  searchTerm: string
  replaceTerm: string
  caseSensitive: boolean
  wholeWord: boolean
  useRegex: boolean
  results: Range[]
  resultIndex: number
  dirty: boolean
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    searchReplace: {
      setSearchTerm: (term: string) => ReturnType
      setReplaceTerm: (term: string) => ReturnType
      setSearchOptions: (
        options: Partial<
          Pick<SearchReplaceStorage, "caseSensitive" | "wholeWord" | "useRegex">
        >
      ) => ReturnType
      goToSearchResult: (offset: number) => ReturnType
      replaceCurrent: () => ReturnType
      replaceAll: () => ReturnType
      clearSearch: () => ReturnType
    }
  }
  interface Storage {
    searchReplace: SearchReplaceStorage
  }
}

const searchReplaceKey = new PluginKey<DecorationSet>("searchReplace")

function buildRegex(storage: SearchReplaceStorage): RegExp | null {
  if (!storage.searchTerm) return null
  let source = storage.useRegex
    ? storage.searchTerm
    : storage.searchTerm.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
  if (storage.wholeWord) source = `\\b${source}\\b`
  try {
    return new RegExp(source, storage.caseSensitive ? "gu" : "gui")
  } catch {
    return null
  }
}

function collectResults(doc: PMNode, regex: RegExp): Range[] {
  const chunks: Array<{ text: string; pos: number }> = []
  let index = 0

  doc.descendants((node, pos) => {
    if (node.isText) {
      const current = chunks[index]
      chunks[index] = current
        ? { text: current.text + node.text, pos: current.pos }
        : { text: node.text ?? "", pos }
    } else {
      index += 1
    }
  })

  const results: Range[] = []
  for (const chunk of chunks.filter(Boolean)) {
    for (const match of chunk.text.matchAll(regex)) {
      if (!match[0] || match.index === undefined) continue
      results.push({
        from: chunk.pos + match.index,
        to: chunk.pos + match.index + match[0].length,
      })
    }
  }
  return results
}

function decorate(results: Range[], activeIndex: number): Decoration[] {
  return results.map((result, index) =>
    Decoration.inline(result.from, result.to, {
      class:
        index === activeIndex
          ? "mavi-search-result mavi-search-result-active"
          : "mavi-search-result",
    })
  )
}

export const SearchReplace = Extension.create<
  Record<string, never>,
  SearchReplaceStorage
>({
  name: "searchReplace",

  addStorage() {
    return {
      searchTerm: "",
      replaceTerm: "",
      caseSensitive: false,
      wholeWord: false,
      useRegex: false,
      results: [],
      resultIndex: 0,
      dirty: false,
    }
  },

  addCommands() {
    return {
      setSearchTerm:
        (term) =>
        ({ editor }) => {
          editor.storage.searchReplace.searchTerm = term
          editor.storage.searchReplace.resultIndex = 0
          editor.storage.searchReplace.dirty = true
          return true
        },

      setReplaceTerm:
        (term) =>
        ({ editor }) => {
          editor.storage.searchReplace.replaceTerm = term
          return true
        },

      setSearchOptions:
        (options) =>
        ({ editor }) => {
          Object.assign(editor.storage.searchReplace, options)
          editor.storage.searchReplace.resultIndex = 0
          editor.storage.searchReplace.dirty = true
          return true
        },

      goToSearchResult:
        (offset) =>
        ({ editor, tr, dispatch }) => {
          const storage = editor.storage.searchReplace
          const total = storage.results.length
          if (!total) return false

          storage.resultIndex = (storage.resultIndex + offset + total) % total
          storage.dirty = true

          const result = storage.results[storage.resultIndex]
          if (dispatch) {
            tr.setSelection(
              TextSelection.create(tr.doc, result.from, result.to)
            ).scrollIntoView()
          }
          return true
        },

      replaceCurrent:
        () =>
        ({ editor, tr, dispatch }) => {
          const storage = editor.storage.searchReplace
          const result = storage.results[storage.resultIndex]
          if (!result) return false
          if (dispatch) {
            tr.insertText(storage.replaceTerm, result.from, result.to)
            storage.dirty = true
          }
          return true
        },

      replaceAll:
        () =>
        ({ editor, tr, dispatch }) => {
          const storage = editor.storage.searchReplace
          if (!storage.results.length) return false
          if (dispatch) {
            for (let i = storage.results.length - 1; i >= 0; i -= 1) {
              const { from, to } = storage.results[i]
              tr.insertText(storage.replaceTerm, from, to)
            }
            storage.resultIndex = 0
            storage.dirty = true
          }
          return true
        },

      clearSearch:
        () =>
        ({ editor }) => {
          editor.storage.searchReplace.searchTerm = ""
          editor.storage.searchReplace.results = []
          editor.storage.searchReplace.resultIndex = 0
          editor.storage.searchReplace.dirty = true
          return true
        },
    }
  },

  addProseMirrorPlugins() {
    const storage = this.storage

    return [
      new Plugin<DecorationSet>({
        key: searchReplaceKey,
        state: {
          init: () => DecorationSet.empty,
          apply(transaction, previous) {
            if (!transaction.docChanged && !storage.dirty) {
              return previous.map(transaction.mapping, transaction.doc)
            }
            storage.dirty = false

            const regex = buildRegex(storage)
            if (!regex) {
              storage.results = []
              return DecorationSet.empty
            }

            storage.results = collectResults(transaction.doc, regex)
            if (storage.resultIndex >= storage.results.length) {
              storage.resultIndex = 0
            }
            return DecorationSet.create(
              transaction.doc,
              decorate(storage.results, storage.resultIndex)
            )
          },
        },
        props: {
          decorations(state) {
            return searchReplaceKey.getState(state)
          },
        },
      }),
    ]
  },
})
