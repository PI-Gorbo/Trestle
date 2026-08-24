<script setup lang="ts">
import { CopyIcon, PlusIcon, XIcon } from '@lucide/vue'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { Program } from '@/composables/usePrograms'

defineProps<{
  programs: readonly Program[]
  activeId: string | null
}>()

const emit = defineEmits<{
  select: [id: string]
  close: [id: string]
  duplicate: [id: string]
  create: []
  rename: [id: string, name: string]
}>()

/** The tab currently being renamed, if any. */
const editingId = ref<string | null>(null)
const draftName = ref('')

const startRename = (program: Program) => {
  editingId.value = program.id
  draftName.value = program.name
}

const commitRename = () => {
  if (!editingId.value) return
  emit('rename', editingId.value, draftName.value)
  editingId.value = null
}
</script>

<template>
  <div class="flex items-stretch gap-1 overflow-x-auto border-b border-border bg-card px-2">
    <div
      v-for="program in programs"
      :key="program.id"
      :class="
        cn(
          'group flex shrink-0 items-center gap-2 border-b-2 px-3 py-2 text-sm transition-colors',
          program.id === activeId
            ? 'border-primary text-foreground'
            : 'border-transparent text-muted-foreground hover:text-foreground',
        )
      "
    >
      <input
        v-if="editingId === program.id"
        v-model="draftName"
        class="w-32 rounded-sm bg-input px-1 font-mono text-sm outline-none ring-1 ring-ring"
        @keydown.enter="commitRename"
        @keydown.esc="editingId = null"
        @blur="commitRename"
      >
      <button
        v-else
        type="button"
        class="cursor-pointer font-mono"
        @click="emit('select', program.id)"
        @dblclick="startRename(program)"
      >
        {{ program.name }}
      </button>

      <span class="flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
        <Button
          variant="ghost"
          size="icon"
          class="size-5"
          :aria-label="`Duplicate ${program.name}`"
          @click="emit('duplicate', program.id)"
        >
          <CopyIcon class="size-3" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          class="size-5"
          :aria-label="`Close ${program.name}`"
          @click="emit('close', program.id)"
        >
          <XIcon class="size-3" />
        </Button>
      </span>
    </div>

    <Button
      variant="ghost"
      size="icon"
      class="my-1 size-7 shrink-0"
      aria-label="New program"
      @click="emit('create')"
    >
      <PlusIcon class="size-4" />
    </Button>
  </div>
</template>
