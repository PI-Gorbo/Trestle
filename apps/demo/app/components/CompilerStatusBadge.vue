<script setup lang="ts">
import { CircleAlertIcon, CircleCheckIcon } from '@lucide/vue'
import { Badge } from '@/components/ui/badge'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { CompilerEngine } from '@/lib/compiler/types'

const props = defineProps<{ engine: CompilerEngine }>()

const isUnavailable = computed(() => props.engine.kind === 'unavailable')
</script>

<template>
  <Tooltip>
    <TooltipTrigger as-child>
      <Badge
        :variant="isUnavailable ? 'destructive' : 'secondary'"
        :class="isUnavailable ? 'gap-1.5 font-mono' : 'gap-1.5 bg-success/15 text-success font-mono'"
      >
        <component :is="isUnavailable ? CircleAlertIcon : CircleCheckIcon" class="size-3" />
        {{ engine.kind === 'wasm' ? `wasm ${engine.version}` : 'COMPILER UNAVAILABLE' }}
      </Badge>
    </TooltipTrigger>
    <TooltipContent class="max-w-xs">
      <p v-if="engine.kind === 'wasm'">
        Running the real Trestle compiler, compiled to WebAssembly.
      </p>
      <p v-else>
        Nothing can be checked or evaluated. Reason: {{ engine.reason }}.
      </p>
    </TooltipContent>
  </Tooltip>
</template>
