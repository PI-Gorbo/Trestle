<script setup lang="ts">
import type * as Monaco from 'monaco-editor/esm/vs/editor/editor.api.js'
import {
  TRESTLE_LANGUAGE_ID,
  TRESTLE_THEME_ID,
  registerTrestleLanguage,
} from '@/lib/monaco/trestle'
import type { Binding, Diagnostic, SourceRange } from '@/lib/compiler/types'

const props = defineProps<{
  /** One Monaco model per program, so undo history and view state survive tab switches. */
  programId: string
  diagnostics: Diagnostic[]
  bindings: Binding[]
}>()

const emit = defineEmits<{ run: [] }>()

const code = defineModel<string>({ required: true })

const host = useTemplateRef<HTMLDivElement>('host')

let monaco: typeof Monaco | null = null
let editor: Monaco.editor.IStandaloneCodeEditor | null = null

const models = new Map<string, Monaco.editor.ITextModel>()
const viewStates = new Map<string, Monaco.editor.ICodeEditorViewState | null>()

/** Suppresses the model-change handler while we are the ones writing to the model. */
let applyingExternalEdit = false

const OWNER = 'trestle'

const modelFor = (id: string, value: string) => {
  const existing = models.get(id)
  if (existing) return existing

  const model = monaco!.editor.createModel(value, TRESTLE_LANGUAGE_ID)
  model.onDidChangeContent(() => {
    if (applyingExternalEdit) return
    code.value = model.getValue()
  })
  models.set(id, model)
  return model
}

const severityOf = (diagnostic: Diagnostic) => {
  const markerSeverity = monaco!.MarkerSeverity
  if (diagnostic.severity === 'warning') return markerSeverity.Warning
  if (diagnostic.severity === 'advice') return markerSeverity.Info
  return markerSeverity.Error
}

/**
 * One marker per diagnostic, anchored to its first label. A diagnostic's remaining labels
 * become related information — that is how `DuplicateBinding` gets to point at the original
 * declaration as well as the redeclaration.
 */
const applyMarkers = () => {
  if (!monaco || !editor) return

  const model = editor.getModel()
  if (!model) return

  const markers = props.diagnostics.flatMap<Monaco.editor.IMarkerData>((diagnostic) => {
    const [primary, ...related] = diagnostic.labels
    // A diagnostic with no labels (a compiler trap, a timeout, or an unbuilt compiler) has
    // nowhere to point. It still shows in the panel; it just gets no squiggle.
    if (!primary) return []

    return [
      {
        severity: severityOf(diagnostic),
        message: diagnostic.help
          ? `${diagnostic.message}\n\n${diagnostic.help}`
          : diagnostic.message,
        code: diagnostic.code ?? undefined,
        source: diagnostic.phase,
        startLineNumber: primary.startLine,
        startColumn: primary.startColumn,
        endLineNumber: primary.endLine,
        endColumn: primary.endColumn,
        relatedInformation: related.map((label) => ({
          resource: model.uri,
          message: label.message ?? diagnostic.message,
          startLineNumber: label.startLine,
          startColumn: label.startColumn,
          endLineNumber: label.endLine,
          endColumn: label.endColumn,
        })),
      },
    ]
  })

  monaco.editor.setModelMarkers(model, OWNER, markers)
}

/** Reveal and select a range — used when a diagnostics or bindings row is clicked. */
const revealRange = (range: SourceRange) => {
  if (!editor) return

  const selection = {
    startLineNumber: range.startLine,
    startColumn: range.startColumn,
    endLineNumber: range.endLine,
    endColumn: range.endColumn,
  }
  editor.setSelection(selection)
  editor.revealRangeInCenterIfOutsideViewport(selection)
  editor.focus()
}

defineExpose({ revealRange })

onMounted(async () => {
  // Imported here rather than at module scope: `monaco-editor` reaches for `window` and
  // `document` while evaluating, so it cannot be present in a module graph that Nuxt might
  // touch outside the browser.
  // Two imports rather than the package root. `editor.api` is the typed API surface;
  // `edcore.main` is a side-effect import that registers the editor's contributions (find,
  // folding, bracket matching, the suggest widget). Together they are everything the default
  // `monaco-editor` entry gives *except* the dozens of built-in language grammars, which we
  // have no use for — this app registers the only language it needs.
  const [monacoModule, EditorWorker] = await Promise.all([
    import('monaco-editor/esm/vs/editor/editor.api.js'),
    import('monaco-editor/esm/vs/editor/editor.worker.js?worker'),
    import('monaco-editor/esm/vs/editor/edcore.main.js'),
  ])

  // Bound locally as well as on the module-scoped `monaco`, because TypeScript cannot keep
  // a mutable module-scoped binding narrowed across the `await` boundaries below.
  const api = monacoModule
  monaco = api

  // Only the base editor worker is registered. Monaco's TypeScript/JSON/CSS language workers
  // are for languages we do not have, and skipping them keeps a large chunk out of the
  // bundle entirely.
  self.MonacoEnvironment = { getWorker: () => new EditorWorker.default() }

  registerTrestleLanguage(api)

  // Bindings carry their inferred type and definition span, so hovers come almost free.
  api.languages.registerHoverProvider(TRESTLE_LANGUAGE_ID, {
    provideHover: (model, position) => {
      const word = model.getWordAtPosition(position)
      if (!word) return null

      const binding = props.bindings.find((candidate) => candidate.name === word.word)
      if (!binding) return null

      return {
        contents: [{ value: `\`\`\`trestle\n${binding.name} : ${binding.type}\n\`\`\`` }],
      }
    },
  })

  editor = api.editor.create(host.value!, {
    model: modelFor(props.programId, code.value),
    theme: TRESTLE_THEME_ID,
    automaticLayout: true,
    fontSize: 14,
    fontFamily: "'JetBrains Mono', 'SF Mono', ui-monospace, monospace",
    fontLigatures: true,
    lineHeight: 1.7,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    padding: { top: 16, bottom: 16 },
    renderLineHighlight: 'line',
    smoothScrolling: true,
    tabSize: 4,
    // The language has no `;`, and a stray one is a syntax error — so do not offer to insert
    // anything the compiler will reject.
    quickSuggestions: false,
    occurrencesHighlight: 'off',
  })

  editor.addCommand(api.KeyMod.CtrlCmd | api.KeyCode.Enter, () => emit('run'))

  applyMarkers()
})

// Switching tabs swaps the model and restores where the cursor was.
watch(
  () => props.programId,
  (id, previous) => {
    if (!editor || !monaco) return

    if (previous) viewStates.set(previous, editor.saveViewState())

    editor.setModel(modelFor(id, code.value))
    const restored = viewStates.get(id)
    if (restored) editor.restoreViewState(restored)

    applyMarkers()
    editor.focus()
  },
)

// An external write to the model — a tab restored from storage, or an example loaded into
// the current tab. Guarded so it does not echo back through the change handler.
watch(code, (next) => {
  const model = models.get(props.programId)
  if (!model || model.getValue() === next) return

  applyingExternalEdit = true
  model.setValue(next)
  applyingExternalEdit = false
})

watch(() => props.diagnostics, applyMarkers, { deep: true })

onBeforeUnmount(() => {
  editor?.dispose()
  for (const model of models.values()) model.dispose()
  models.clear()
})
</script>

<template>
  <div ref="host" class="h-full w-full" />
</template>
