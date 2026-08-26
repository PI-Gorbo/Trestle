<script setup lang="ts">
import { ChevronDownIcon, ListChecksIcon, PlayIcon } from '@lucide/vue'
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
import { EXAMPLES, EXAMPLE_GROUPS } from '@/lib/examples'
import { REPO_URL } from '@/lib/links'
import type { SourceRange } from '@/lib/compiler/types'

/** Long enough that a burst of typing produces one compile, not one per keystroke. */
const CHECK_DEBOUNCE_MS = 400

const { programs, activeId, active, create, update, rename, duplicate, remove, select, openExample }
  = usePrograms()
const { engine, stateFor, check, run, forget } = useCompiler()

const editor = useTemplateRef<{ revealRange: (range: SourceRange) => void }>('editor')
const rightPanel = ref<'output' | 'diagnostics' | 'bindings'>('output')
const featuresOpen = ref(false)

// The dropdown is long enough now that a flat list is hard to scan, so it is sectioned. The
// order comes from `EXAMPLE_GROUPS`; a group with no examples simply does not render.
const groupedExamples = EXAMPLE_GROUPS.map((title) => ({
  title,
  examples: EXAMPLES.filter((example) => example.group === title),
})).filter((group) => group.examples.length > 0)

const compilerReady = computed(() => engine.value.kind === 'wasm')

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
          <DropdownMenuContent align="end" class="max-h-[70vh] w-80 overflow-y-auto">
            <template v-for="(group, index) in groupedExamples" :key="group.title">
              <DropdownMenuSeparator v-if="index > 0" />
              <DropdownMenuLabel>{{ group.title }}</DropdownMenuLabel>
              <DropdownMenuItem
                v-for="example in group.examples"
                :key="example.name"
                class="flex-col items-start gap-0.5"
                @select="openExample(example.name)"
              >
                <span class="font-mono text-sm">{{ example.name }}.trsl</span>
                <span class="text-xs text-muted-foreground">{{ example.description }}</span>
              </DropdownMenuItem>
            </template>
          </DropdownMenuContent>
        </DropdownMenu>

        <Button variant="outline" size="sm" class="gap-1.5" @click="featuresOpen = true">
          <ListChecksIcon class="size-3.5" />
          Features
        </Button>

        <Button as-child variant="outline" size="sm" class="gap-1.5">
          <a :href="REPO_URL" target="_blank" rel="noreferrer noopener">
            <GithubMark class="size-3.5" />
            GitHub
          </a>
        </Button>

        <Button
          size="sm"
          class="gap-1.5"
          :disabled="!active || !compilerReady"
          @click="runActive"
        >
          <PlayIcon class="size-3.5" />
          Run
        </Button>
      </header>

      <FeaturesDialog v-model:open="featuresOpen" @open-example="openExample" />

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
              <OutputPanel v-if="state" :state="state" :engine="engine" />
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
