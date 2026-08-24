<script setup lang="ts">
import type { Binding } from '@/lib/compiler/types'

defineProps<{ bindings: Binding[] }>()

const emit = defineEmits<{ reveal: [binding: Binding] }>()
</script>

<template>
  <div class="h-full overflow-y-auto">
    <p v-if="bindings.length === 0" class="p-4 text-sm text-muted-foreground">
      No bindings — run a check on a program that type-checks to see its inferred types.
    </p>

    <ul v-else class="divide-y divide-border">
      <li v-for="binding in bindings" :key="`${binding.name}-${binding.startLine}`">
        <button
          type="button"
          class="flex w-full cursor-pointer items-baseline gap-3 p-3 text-left font-mono text-sm transition-colors hover:bg-accent/50"
          @click="emit('reveal', binding)"
        >
          <span class="text-foreground">{{ binding.name }}</span>
          <span class="text-muted-foreground">:</span>
          <span class="flex-1 text-secondary">{{ binding.type }}</span>
          <span class="text-xs text-muted-foreground">{{ binding.startLine }}</span>
        </button>
      </li>
    </ul>
  </div>
</template>
