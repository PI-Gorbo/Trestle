<script setup lang="ts">
import { ChevronDownIcon, PlayIcon } from '@lucide/vue'
import { useDebounceFn } from '@vueuse/core'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { TooltipProvider } from '@/components/ui/tooltip'
import { EXAMPLES } from '@/lib/examples'
import type { SourceRange } from '@/lib/compiler/types'

/** Long enough that a burst of typing produces one compile, not one per keystroke. */
const CHECK_DEBOUNCE_MS = 400

const { programs, activeId, active, create, update, rename, duplicate, remove, select, openExample }
  = usePrograms()
const { engine, stateFor, check, run, forget } = useCompiler()

const editor = useTemplateRef<{ revealRange: (range: SourceRange) => void }>('editor')
const rightPanel = ref<'output' | 'diagnostics' | 'bindings'>('output')

const state = computed(() => (active.value ? stateFor(active.value.id) : null))
const diagnostics = computed(() => state.value?.diagnostics ?? [])
const bindings = computed(() => state.value?.bindings ?? [])

const errorCount = computed(
  () => diagnostics.value.filter((diagnostic) => diagnostic.severity === 'error').length,
)

const code = computed({
  get: () => active.value?.code ?? '',
  set: (next: string) => {
    if (active.value) update(active.value.id, next)
  },
})

const runActive = () => {
  if (!active.value) return
  rightPanel.value = 'output'
  void run(active.value.id, active.value.code)
}

const closeProgram = (id: string) => {
  forget(id)
  remove(id)
}

const reveal = (range: SourceRange) => editor.value?.revealRange(range)

const requestCheck = useDebounceFn(() => {
  if (!active.value) return
  void check(active.value.id, active.value.code)
}, CHECK_DEBOUNCE_MS)

// Check on edit, and on tab switch so a restored tab shows its markers immediately.
watch(
  () => [active.value?.id, active.value?.code] as const,
  ([id]) => {
    if (id) requestCheck()
  },
  { immediate: true },
)

// A failed run should surface its errors without the user hunting for the tab.
watch(errorCount, (count, previous) => {
  if (count > 0 && previous === 0) rightPanel.value = 'diagnostics'
})
</script>

<template>
  <TooltipProvider>
    <div class="flex h-screen flex-col bg-background">
      <header class="flex items-center gap-3 border-b border-border px-4 py-2.5">
        <h1 class="font-mono text-sm font-semibold tracking-tight">
          trestle<span class="text-primary">/</span>playground
        </h1>

        <CompilerStatusBadge :engine="engine" />

        <div class="flex-1" />

        <DropdownMenu>
          <DropdownMenuTrigger as-child>
            <Button variant="outline" size="sm" class="gap-1.5">
              Examples
              <ChevronDownIcon class="size-3.5" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" class="w-80">
            <DropdownMenuLabel>From the conformance corpus</DropdownMenuLabel>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              v-for="example in EXAMPLES"
              :key="example.name"
              class="flex-col items-start gap-0.5"
              @select="openExample(example.name)"
            >
              <span class="font-mono text-sm">{{ example.name }}.trsl</span>
              <span class="text-xs text-muted-foreground">{{ example.description }}</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>

        <Button size="sm" class="gap-1.5" :disabled="!active" @click="runActive">
          <PlayIcon class="size-3.5" />
          Run
        </Button>
      </header>

      <ProgramTabs
        :programs="programs"
        :active-id="activeId"
        @select="select"
        @close="closeProgram"
        @duplicate="duplicate"
        @create="create()"
        @rename="rename"
      />

      <ResizablePanelGroup direction="horizontal" class="flex-1">
        <ResizablePanel :default-size="60" :min-size="30">
          <TrestleEditor
            v-if="active"
            ref="editor"
            v-model="code"
            :program-id="active.id"
            :diagnostics="diagnostics"
            :bindings="bindings"
            @run="runActive"
          />
        </ResizablePanel>

        <ResizableHandle with-handle />

        <ResizablePanel :default-size="40" :min-size="20">
          <Tabs v-model="rightPanel" class="flex h-full flex-col gap-0">
            <TabsList class="w-full justify-start rounded-none border-b border-border bg-card px-2">
              <TabsTrigger value="output">Output</TabsTrigger>
              <TabsTrigger value="diagnostics" class="gap-1.5">
                Diagnostics
                <Badge v-if="errorCount > 0" variant="destructive" class="h-4 px-1.5 text-[10px]">
                  {{ errorCount }}
                </Badge>
              </TabsTrigger>
              <TabsTrigger value="bindings">Bindings</TabsTrigger>
            </TabsList>

            <TabsContent value="output" class="mt-0 min-h-0 flex-1">
              <OutputPanel v-if="state" :state="state" />
            </TabsContent>
            <TabsContent value="diagnostics" class="mt-0 min-h-0 flex-1">
              <DiagnosticsPanel :diagnostics="diagnostics" @reveal="reveal" />
            </TabsContent>
            <TabsContent value="bindings" class="mt-0 min-h-0 flex-1">
              <BindingsPanel :bindings="bindings" @reveal="reveal" />
            </TabsContent>
          </Tabs>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  </TooltipProvider>
</template>
