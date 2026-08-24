<script setup lang="ts">
import { CircleAlertIcon, CircleXIcon, InfoIcon } from '@lucide/vue'
import { Match } from 'effect'
import { Badge } from '@/components/ui/badge'
import type { Diagnostic, Label, Severity } from '@/lib/compiler/types'

defineProps<{ diagnostics: Diagnostic[] }>()

const emit = defineEmits<{ reveal: [label: Label] }>()

const iconFor = Match.type<Severity>().pipe(
  Match.when('error', () => CircleXIcon),
  Match.when('warning', () => CircleAlertIcon),
  Match.when('advice', () => InfoIcon),
  Match.exhaustive,
)

const colourFor = Match.type<Severity>().pipe(
  Match.when('error', () => 'text-destructive'),
  Match.when('warning', () => 'text-warning'),
  Match.when('advice', () => 'text-secondary'),
  Match.exhaustive,
)
</script>

<template>
  <div class="h-full overflow-y-auto">
    <p v-if="diagnostics.length === 0" class="p-4 text-sm text-muted-foreground">
      No diagnostics.
    </p>

    <ul v-else class="divide-y divide-border">
      <li v-for="(diagnostic, index) in diagnostics" :key="`${diagnostic.code}-${index}`">
        <button
          type="button"
          class="flex w-full cursor-pointer items-start gap-3 p-3 text-left transition-colors hover:bg-accent/50"
          :disabled="!diagnostic.labels[0]"
          @click="diagnostic.labels[0] && emit('reveal', diagnostic.labels[0])"
        >
          <component
            :is="iconFor(diagnostic.severity)"
            :class="['mt-0.5 size-4 shrink-0', colourFor(diagnostic.severity)]"
          />

          <div class="min-w-0 flex-1 space-y-1.5">
            <div class="flex flex-wrap items-center gap-2">
              <Badge variant="outline" class="font-mono text-[10px] uppercase">
                {{ diagnostic.phase }}
              </Badge>
              <code v-if="diagnostic.code" class="font-mono text-xs text-muted-foreground">
                {{ diagnostic.code }}
              </code>
              <span
                v-if="diagnostic.labels[0]"
                class="font-mono text-xs text-muted-foreground"
              >
                {{ diagnostic.labels[0].startLine }}:{{ diagnostic.labels[0].startColumn }}
              </span>
            </div>

            <p class="text-sm text-foreground">{{ diagnostic.message }}</p>

            <p v-if="diagnostic.help" class="text-xs text-muted-foreground">
              {{ diagnostic.help }}
            </p>

            <!--
              Secondary labels carry the other half of the story — `DuplicateBinding`'s
              "first declared here", for instance — so they are worth listing rather than
              leaving to the editor's related-information popup.
            -->
            <ul v-if="diagnostic.labels.length > 1" class="space-y-0.5 pt-0.5">
              <li
                v-for="label in diagnostic.labels.slice(1)"
                :key="label.offset"
                class="font-mono text-xs text-muted-foreground"
              >
                {{ label.startLine }}:{{ label.startColumn }} — {{ label.message }}
              </li>
            </ul>
          </div>
        </button>
      </li>
    </ul>
  </div>
</template>
