<script setup lang="ts">
import { LoaderIcon } from '@lucide/vue'
import type { CompileState } from '@/composables/useCompiler'

const props = defineProps<{ state: CompileState }>()

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
    <div v-if="state.status === 'compiling'" class="flex items-center gap-2 text-muted-foreground">
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

    <!-- An advice-only batch: the mock declining to evaluate, most often. -->
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
