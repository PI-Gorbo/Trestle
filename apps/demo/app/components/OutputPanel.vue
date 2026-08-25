<script setup lang="ts">
import { LoaderIcon } from '@lucide/vue'
import type { CompileState } from '@/composables/useCompiler'
import type { CompilerEngine } from '@/lib/compiler/types'

const props = defineProps<{ state: CompileState; engine: CompilerEngine }>()

/**
 * The phase that stopped the pipeline. Trestle fails fast between phases, so every
 * diagnostic in a batch comes from the same one and reading the first is enough.
 */
const failedPhase = computed(() => props.state.diagnostics[0]?.phase ?? null)

const errorCount = computed(
  () => props.state.diagnostics.filter((diagnostic) => diagnostic.severity === 'error').length,
)
</script>

<template>
  <div class="h-full overflow-y-auto p-4 font-mono text-sm">
    <!--
      Checked before `status`, because without a compiler every other state is a description of
      something that never ran. The header badge says the same thing; this says what to do.
    -->
    <div v-if="engine.kind === 'unavailable'" class="space-y-1">
      <p class="text-destructive">The compiler is unavailable.</p>
      <p class="text-xs text-muted-foreground">{{ engine.reason }}.</p>
      <p class="text-xs text-muted-foreground">
        Build it with <span class="text-foreground">pnpm build:wasm</span>, then reload.
      </p>
    </div>

    <div
      v-else-if="state.status === 'compiling'"
      class="flex items-center gap-2 text-muted-foreground"
    >
      <LoaderIcon class="size-4 animate-spin" />
      Compiling…
    </div>

    <div v-else-if="state.status === 'idle'" class="text-muted-foreground">
      Press <kbd class="rounded-sm bg-muted px-1.5 py-0.5 text-xs">⌘↵</kbd> or hit Run.
    </div>

    <div v-else-if="errorCount > 0" class="space-y-1">
      <p class="text-destructive">
        {{ errorCount }} {{ errorCount === 1 ? 'error' : 'errors' }} in
        <span class="text-muted-foreground">{{ failedPhase }}</span>
      </p>
      <p class="text-xs text-muted-foreground">See the Diagnostics tab.</p>
    </div>

    <!-- Nothing has been evaluated yet, or the batch is advice rather than errors. -->
    <div v-else-if="state.value === null" class="space-y-1">
      <p v-for="(diagnostic, index) in state.diagnostics" :key="index" class="text-muted-foreground">
        {{ diagnostic.message }}
        <span v-if="diagnostic.help" class="block text-xs">{{ diagnostic.help }}</span>
      </p>
      <p v-if="state.diagnostics.length === 0" class="text-success">
        Checks passed. Hit Run to evaluate.
      </p>
    </div>

    <div v-else class="space-y-1">
      <p class="text-lg text-foreground">{{ state.value }}</p>
      <p class="text-secondary">: {{ state.valueType }}</p>
    </div>
  </div>
</template>
