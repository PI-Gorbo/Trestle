/*
 * Deliberately a *script*, not a module: it has no top-level `import` or `export`. Inside a
 * module file, `declare module 'x'` means *augment* the existing module `x`, which does
 * nothing for a package entry that ships no types. In a script it declares the module
 * outright, which is what the entry below needs.
 */

/** Monaco reads its worker factory off the global before any editor is created. */
interface Window {
  MonacoEnvironment?: import('monaco-editor/esm/vs/editor/editor.api.js').Environment
}

/**
 * `edcore.main` is the editor's contributions (find, folding, bracket matching, the suggest
 * widget) without any of the bundled language grammars. It is the entry that keeps the
 * bundle lean, but it is the one entry Monaco ships no `.d.ts` for — only `editor.api` and
 * `editor.main` have those. It re-exports exactly `editor.api`'s surface, so say so.
 */
declare module 'monaco-editor/esm/vs/editor/edcore.main.js' {
  export * from 'monaco-editor/esm/vs/editor/editor.api.js'
}
