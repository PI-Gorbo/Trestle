<script setup lang="ts">
/**
 * The language at a glance: what ships today, what is planned, and why.
 *
 * Shipped rows that name an example are buttons — clicking one opens that program in a tab
 * and closes the dialog, so the catalogue doubles as a way into the playground rather than
 * being a wall of text beside it. The data, and the rule about where it comes from, live in
 * `@/lib/features`.
 */
import { ArrowRightIcon } from '@lucide/vue'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogScrollContent,
  DialogTitle,
} from '@/components/ui/dialog'
import { FEATURE_GROUPS } from '@/lib/features'
import { REPO_URL } from '@/lib/links'

const open = defineModel<boolean>('open', { required: true })

const emit = defineEmits<{ openExample: [name: string] }>()

const shipped = FEATURE_GROUPS.flatMap((group) => group.features).filter(
  (feature) => feature.status === 'shipped',
).length

const select = (name: string) => {
  emit('openExample', name)
  open.value = false
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogScrollContent class="sm:max-w-3xl">
      <DialogHeader>
        <DialogTitle>Trestle language features</DialogTitle>
        <DialogDescription>
          {{ shipped }} features ship today — each one has a runnable example, and clicking a
          row opens it. The rest is what the language is being built toward.
        </DialogDescription>
      </DialogHeader>

      <div class="space-y-6">
        <section v-for="group in FEATURE_GROUPS" :key="group.title">
          <h3
            class="mb-2 font-mono text-xs font-semibold tracking-wide text-muted-foreground uppercase"
          >
            {{ group.title }}
          </h3>

          <ul class="divide-y divide-border rounded-md border border-border">
            <li v-for="feature in group.features" :key="feature.name">
              <component
                :is="feature.example ? 'button' : 'div'"
                :type="feature.example ? 'button' : undefined"
                class="group flex w-full items-start gap-3 p-3 text-left"
                :class="
                  feature.example
                    ? 'cursor-pointer transition-colors hover:bg-accent/50'
                    : undefined
                "
                @click="feature.example && select(feature.example)"
              >
                <span
                  class="mt-1.5 size-2 shrink-0 rounded-full border"
                  :class="
                    feature.status === 'shipped'
                      ? 'border-primary bg-primary'
                      : 'border-muted-foreground'
                  "
                  :aria-label="feature.status"
                />

                <div class="min-w-0 flex-1 space-y-1">
                  <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
                    <span class="text-sm font-medium">{{ feature.name }}</span>
                    <code class="font-mono text-xs text-muted-foreground">
                      {{ feature.syntax }}
                    </code>
                  </div>
                  <p v-if="feature.note" class="text-xs text-muted-foreground">
                    {{ feature.note }}
                  </p>
                </div>

                <ArrowRightIcon
                  v-if="feature.example"
                  class="mt-1 size-3.5 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
                />
              </component>
            </li>
          </ul>
        </section>
      </div>

      <DialogFooter class="sm:items-center sm:justify-between">
        <p class="flex items-center gap-4 text-xs text-muted-foreground">
          <span class="flex items-center gap-1.5">
            <span class="size-2 rounded-full border border-primary bg-primary" />
            ships today
          </span>
          <span class="flex items-center gap-1.5">
            <span class="size-2 rounded-full border border-muted-foreground" />
            planned
          </span>
        </p>

        <Button as-child variant="outline" size="sm" class="gap-1.5">
          <a :href="REPO_URL" target="_blank" rel="noreferrer noopener">
            <GithubMark class="size-3.5" />
            View on GitHub
          </a>
        </Button>
      </DialogFooter>
    </DialogScrollContent>
  </Dialog>
</template>
