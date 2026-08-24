/**
 * Trestle language support for Monaco.
 *
 * The tokenizer is derived from `crates/trestle/src/parse/trestle.pest` and the builtin type
 * names from `crates/trestle/src/prelude.rs`. It is a syntax highlighter, not a parser — the
 * real compiler owns correctness, and everything it finds arrives as a marker.
 */

import type * as Monaco from 'monaco-editor/esm/vs/editor/editor.api.js'

export const TRESTLE_LANGUAGE_ID = 'trestle'
export const TRESTLE_THEME_ID = 'trestle-dark'

/** `expr` in the grammar: the keywords that introduce an expression form. */
const KEYWORDS = ['let', 'if', 'else', 'type']

/** `literal` in the grammar. `unit` is a literal, not a type name, in expression position. */
const LITERALS = ['true', 'false', 'unit']

/** `PRELUDE_TYPES` — the type bindings every program starts with. */
const BUILTIN_TYPES = ['Int', 'Float', 'String', 'Bool', 'Unit']

const tokenizer: Monaco.languages.IMonarchLanguage = {
  keywords: KEYWORDS,
  literals: LITERALS,
  builtinTypes: BUILTIN_TYPES,

  // Longest first: the PEG tries alternatives in order and so must Monarch, or `<` would
  // shadow `<=` and `!` would shadow `!=`.
  operators: [
    '|>',
    '=>',
    '&&',
    '||',
    '==',
    '!=',
    '<=',
    '>=',
    '<',
    '>',
    '+',
    '-',
    '*',
    '/',
    '!',
    '=',
  ],

  symbols: /[=><!|&+\-*/]+/,
  escapes: /\\./,

  /*
   * `[A-Za-z_]` is spelled out rather than folded into an `i` flag, and the lint asking for
   * that flag is turned off here. Monarch recompiles every rule's `source` with its own
   * flags, so an `i` on the literal is not guaranteed to survive — and if it were dropped,
   * `[a-z_]` would stop matching capitalised identifiers like `Int` altogether.
   */
  /* eslint-disable regexp/use-ignore-case */
  tokenizer: {
    root: [
      // A `:` before an identifier is a type annotation (`type_declaration` in the grammar),
      // which is the only place a bare identifier names a type. Matching the colon and the
      // name together is what lets `x: Int` colour `Int` as a type without a symbol table.
      [/(:)(\s*)([A-Za-z_]\w*)/, ['delimiter', 'white', 'type']],

      [
        /[A-Za-z_]\w*/,
        {
          cases: {
            '@keywords': 'keyword',
            '@literals': 'keyword.literal',
            '@builtinTypes': 'type',
            '@default': 'identifier',
          },
        },
      ],

      { include: '@whitespace' },

      [/[{}()]/, '@brackets'],

      // `float` is `int ~ "." ~ int`, so it must be tried before `int`.
      [/\d+\.\d+/, 'number.float'],
      [/\d+/, 'number'],

      [/@symbols/, { cases: { '@operators': 'operator', '@default': '' } }],

      [/[,:.]/, 'delimiter'],

      [/"/, { token: 'string.quote', next: '@string' }],
    ],

    whitespace: [
      [/\s+/, 'white'],
      [/\/\/.*$/, 'comment'],
    ],

    string: [
      [/@escapes/, 'string.escape'],
      [/[^\\"]+/, 'string'],
      [/"/, { token: 'string.quote', next: '@pop' }],
    ],
  },
  /* eslint-enable regexp/use-ignore-case */
}

const configuration: Monaco.languages.LanguageConfiguration = {
  comments: { lineComment: '//' },
  brackets: [
    ['{', '}'],
    ['(', ')'],
  ],
  autoClosingPairs: [
    { open: '{', close: '}' },
    { open: '(', close: ')' },
    { open: '"', close: '"', notIn: ['string'] },
  ],
  surroundingPairs: [
    { open: '{', close: '}' },
    { open: '(', close: ')' },
    { open: '"', close: '"' },
  ],
}

/**
 * Colours are the literal values behind the app's CSS custom properties rather than
 * `var(--…)` references, because Monaco parses these into its own colour model and cannot
 * resolve CSS variables. Keep them in step with `app/assets/css/main.css`.
 */
const theme: Monaco.editor.IStandaloneThemeData = {
  base: 'vs-dark',
  inherit: true,
  rules: [
    { token: 'keyword', foreground: 'e8955a', fontStyle: 'bold' },
    { token: 'keyword.literal', foreground: 'e8955a' },
    { token: 'type', foreground: '6fb0ad' },
    { token: 'identifier', foreground: 'd6d3d1' },
    { token: 'number', foreground: 'c8b6e2' },
    { token: 'number.float', foreground: 'c8b6e2' },
    { token: 'string', foreground: '9ec78d' },
    { token: 'string.quote', foreground: '9ec78d' },
    { token: 'string.escape', foreground: 'e8955a' },
    { token: 'comment', foreground: '7a736e', fontStyle: 'italic' },
    { token: 'operator', foreground: 'a8a29e' },
    { token: 'delimiter', foreground: 'a8a29e' },
  ],
  colors: {
    'editor.background': '#141414',
    'editor.foreground': '#d6d3d1',
    'editorLineNumber.foreground': '#57534e',
    'editorLineNumber.activeForeground': '#a8a29e',
    'editor.selectionBackground': '#2f2f2f',
    'editor.lineHighlightBackground': '#1c1c1c',
    'editorCursor.foreground': '#e8955a',
    'editorIndentGuide.background1': '#262626',
    'editorWidget.background': '#1c1c1c',
    'editorWidget.border': '#2f2f2f',
    'editorHoverWidget.background': '#1c1c1c',
  },
}

let registered = false

/** Idempotent: Monaco throws if a language id is registered twice. */
export const registerTrestleLanguage = (monaco: typeof Monaco) => {
  if (registered) return
  registered = true

  monaco.languages.register({ id: TRESTLE_LANGUAGE_ID, extensions: ['.trsl'] })
  monaco.languages.setMonarchTokensProvider(TRESTLE_LANGUAGE_ID, tokenizer)
  monaco.languages.setLanguageConfiguration(TRESTLE_LANGUAGE_ID, configuration)
  monaco.editor.defineTheme(TRESTLE_THEME_ID, theme)
}
