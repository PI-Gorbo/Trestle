<script setup lang="ts">
import { CircleAlertIcon, CircleCheckIcon } from '@lucide/vue'
import { Badge } from '@/components/ui/badge'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { CompilerEngine } from '@/lib/compiler/types'

const props = defineProps<{ engine: CompilerEngine }>()

const isMock = computed(() => props.engine.kind === 'mock')
</script>

<template>
  <Tooltip>
    <TooltipTrigger as-child>
      <Badge
        :variant="isMock ? 'outline' : 'secondary'"
        :class="
          isMock
            ? 'gap-1.5 border-warning/50 bg-warning/10 text-warning font-mono'
            : 'gap-1.5 bg-success/15 text-success font-mono'
        "
      >
        <component :is="isMock ? CircleAlertIcon : CircleCheckIcon" class="size-3" />
        {{ engine.kind === 'wasm' ? `wasm ${engine.version}` : 'MOCK COMPILER' }}
      </Badge>
    </TooltipTrigger>
    <TooltipContent class="max-w-xs">
      <p v-if="engine.kind === 'wasm'">
        Running the real Trestle compiler, compiled to WebAssembly.
      </p>
      <p v-else>
        The real compiler is unavailable, so only lexical checks run and programs cannot be
        evaluated. Reason: {{ engine.reason }}.
      </p>
    </TooltipContent>
  </Tooltip>
</template>
