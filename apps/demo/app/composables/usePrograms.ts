/**
 * The open programs, persisted to localStorage.
 *
 * Hand-rolled rather than reached for via a persistence plugin, matching the convention in
 * the other apps: a module-scoped key, a client guard, JSON in a `try`/`catch`, and a version
 * in the payload so a future shape change migrates or resets instead of throwing on load.
 *
 * State is module-scoped, not per-call, so every component sees the same tabs.
 */

import { useDebounceFn } from '@vueuse/core'
import { BLANK_PROGRAM, EXAMPLES } from '@/lib/examples'

const STORAGE_KEY = 'trestle-demo/programs/v1'

/** Long enough not to write on every keystroke, short enough to survive a quick tab close. */
const PERSIST_DEBOUNCE_MS = 300

export type Program = {
  id: string
  name: string
  code: string
  createdAt: string
  updatedAt: string
}

type PersistedState = {
  version: 1
  programs: Program[]
  activeId: string | null
}

const newId = () =>
  // `crypto.randomUUID` needs a secure context; a deployed playground on plain HTTP would
  // otherwise lose every tab it tried to create.
  globalThis.crypto?.randomUUID?.() ?? `program-${Date.now()}-${Math.random().toString(36).slice(2)}`

const now = () => new Date().toISOString()

const makeProgram = (name: string, code: string): Program => ({
  id: newId(),
  name,
  code,
  createdAt: now(),
  updatedAt: now(),
})

/** First-run contents: a scratch tab plus the two examples that best show the language off. */
const seed = (): Program[] => [
  makeProgram('scratch.trsl', BLANK_PROGRAM),
  ...EXAMPLES.filter((example) => example.name === 'currying' || example.name === 'shadowing').map(
    (example) => makeProgram(`${example.name}.trsl`, example.code),
  ),
]

const isProgram = (value: unknown): value is Program => {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Partial<Program>
  return (
    typeof candidate.id === 'string' &&
    typeof candidate.name === 'string' &&
    typeof candidate.code === 'string'
  )
}

const load = (): PersistedState | null => {
  if (import.meta.server) return null

  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    if (!raw) return null

    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== 'object' || parsed === null) return null

    const candidate = parsed as Partial<PersistedState>
    if (candidate.version !== 1 || !Array.isArray(candidate.programs)) return null

    const programs = candidate.programs.filter(isProgram)
    if (programs.length === 0) return null

    return {
      version: 1,
      programs,
      activeId: typeof candidate.activeId === 'string' ? candidate.activeId : null,
    }
  } catch {
    // Corrupt or unreadable storage (quota, private mode, a hand-edited value). Falling back
    // to the seed loses the tabs but keeps the app usable, which is the better trade.
    return null
  }
}

const programs = ref<Program[]>([])
const activeId = ref<string | null>(null)
let hydrated = false

const persist = useDebounceFn(() => {
  if (import.meta.server) return

  try {
    const state: PersistedState = {
      version: 1,
      programs: programs.value,
      activeId: activeId.value,
    }
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
  } catch {
    // Storage full or blocked. Editing must not break because we cannot save.
  }
}, PERSIST_DEBOUNCE_MS)

export const usePrograms = () => {
  if (!hydrated && import.meta.client) {
    hydrated = true
    const stored = load()
    programs.value = stored?.programs ?? seed()
    activeId.value =
      stored?.activeId && programs.value.some((program) => program.id === stored.activeId)
        ? stored.activeId
        : (programs.value[0]?.id ?? null)
  }

  watch([programs, activeId], persist, { deep: true })

  const active = computed(
    () => programs.value.find((program) => program.id === activeId.value) ?? null,
  )

  /** Make `base` unique among the open tabs by suffixing a counter. */
  const uniqueName = (base: string) => {
    const taken = new Set(programs.value.map((program) => program.name))
    if (!taken.has(base)) return base

    const stem = base.replace(/\.trsl$/, '')
    let index = 2
    while (taken.has(`${stem}-${index}.trsl`)) index += 1
    return `${stem}-${index}.trsl`
  }

  const create = (name = 'untitled.trsl', code = BLANK_PROGRAM) => {
    const program = makeProgram(uniqueName(name), code)
    programs.value = [...programs.value, program]
    activeId.value = program.id
    return program
  }

  const update = (id: string, code: string) => {
    programs.value = programs.value.map((program) =>
      program.id === id ? { ...program, code, updatedAt: now() } : program,
    )
  }

  const rename = (id: string, name: string) => {
    const trimmed = name.trim()
    if (!trimmed) return

    programs.value = programs.value.map((program) =>
      program.id === id
        ? { ...program, name: program.name === trimmed ? trimmed : uniqueName(trimmed), updatedAt: now() }
        : program,
    )
  }

  const duplicate = (id: string) => {
    const original = programs.value.find((program) => program.id === id)
    if (!original) return

    const copy = makeProgram(uniqueName(original.name), original.code)
    programs.value = [...programs.value, copy]
    activeId.value = copy.id
  }

  const remove = (id: string) => {
    const index = programs.value.findIndex((program) => program.id === id)
    if (index === -1) return

    programs.value = programs.value.filter((program) => program.id !== id)

    // Closing the last tab would leave nothing to edit, so always keep one open.
    if (programs.value.length === 0) {
      const fresh = makeProgram('scratch.trsl', BLANK_PROGRAM)
      programs.value = [fresh]
      activeId.value = fresh.id
      return
    }

    if (activeId.value === id) {
      // Select the neighbour to the left, the way editors do.
      activeId.value = programs.value[Math.max(0, index - 1)]!.id
    }
  }

  const openExample = (name: string) => {
    const example = EXAMPLES.find((candidate) => candidate.name === name)
    if (!example) return

    const alreadyOpen = programs.value.find((program) => program.name === `${example.name}.trsl`)
    if (alreadyOpen) {
      activeId.value = alreadyOpen.id
      return
    }

    create(`${example.name}.trsl`, example.code)
  }

  const select = (id: string) => {
    activeId.value = id
  }

  return {
    programs: readonly(programs),
    activeId: readonly(activeId),
    active,
    create,
    update,
    rename,
    duplicate,
    remove,
    select,
    openExample,
  }
}
