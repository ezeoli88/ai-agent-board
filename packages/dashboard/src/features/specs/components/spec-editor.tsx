'use client'

import { useState, useCallback, useMemo } from 'react'
import { Eye, Pencil, Save, Loader2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { cn } from '@/lib/utils'
import { useUpdateSpec } from '../hooks/use-spec-mutations'
import { toast } from 'sonner'

type EditorMode = 'edit' | 'preview'

interface SpecEditorProps {
  specId: string
  /** The content to display/edit. Falls back to final_spec, then draft_spec. */
  content: string | null
  /** Whether editing is allowed (e.g., disabled for approved/terminal specs) */
  readOnly?: boolean
}

/**
 * Renders a basic markdown preview. Handles headings, bold, italic, inline code,
 * fenced code blocks, unordered/ordered lists, horizontal rules, links, and
 * blockquotes. Not a full-featured markdown parser, but sufficient for spec
 * documents without pulling in a dependency.
 */
function renderMarkdownToHtml(markdown: string): string {
  let html = markdown

  // Escape HTML entities to prevent XSS
  html = html
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')

  // Fenced code blocks (```lang\n...\n```)
  html = html.replace(
    /```(\w*)\n([\s\S]*?)```/g,
    (_match, _lang, code) =>
      `<pre class="rounded-md bg-muted p-4 overflow-x-auto text-sm font-mono my-3"><code>${code.trim()}</code></pre>`
  )

  // Inline code (`...`)
  html = html.replace(
    /`([^`]+)`/g,
    '<code class="rounded bg-muted px-1.5 py-0.5 text-sm font-mono">$1</code>'
  )

  // Headings (# through ####)
  html = html.replace(
    /^#### (.+)$/gm,
    '<h4 class="text-base font-semibold mt-4 mb-2">$1</h4>'
  )
  html = html.replace(
    /^### (.+)$/gm,
    '<h3 class="text-lg font-semibold mt-5 mb-2">$1</h3>'
  )
  html = html.replace(
    /^## (.+)$/gm,
    '<h2 class="text-xl font-semibold mt-6 mb-3">$1</h2>'
  )
  html = html.replace(
    /^# (.+)$/gm,
    '<h1 class="text-2xl font-bold mt-6 mb-3">$1</h1>'
  )

  // Horizontal rule
  html = html.replace(
    /^---+$/gm,
    '<hr class="my-4 border-border" />'
  )

  // Bold (**text** or __text__)
  html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
  html = html.replace(/__(.+?)__/g, '<strong>$1</strong>')

  // Italic (*text* or _text_) - avoid matching inside words with underscores
  html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>')
  html = html.replace(/(?<!\w)_([^_]+)_(?!\w)/g, '<em>$1</em>')

  // Links [text](url)
  html = html.replace(
    /\[([^\]]+)\]\(([^)]+)\)/g,
    '<a href="$2" target="_blank" rel="noopener noreferrer" class="text-primary underline underline-offset-2 hover:text-primary/80">$1</a>'
  )

  // Blockquotes (&gt; because we already escaped HTML)
  html = html.replace(
    /^&gt; (.+)$/gm,
    '<blockquote class="border-l-4 border-border pl-4 italic text-muted-foreground my-2">$1</blockquote>'
  )

  // Unordered lists (- item or * item)
  html = html.replace(
    /^(?:[*-]) (.+)$/gm,
    '<li class="ml-4 list-disc">$1</li>'
  )

  // Ordered lists (1. item)
  html = html.replace(
    /^\d+\. (.+)$/gm,
    '<li class="ml-4 list-decimal">$1</li>'
  )

  // Wrap consecutive <li> elements in <ul> / <ol>
  html = html.replace(
    /((?:<li class="ml-4 list-disc">.*<\/li>\n?)+)/g,
    '<ul class="my-2 space-y-1">$1</ul>'
  )
  html = html.replace(
    /((?:<li class="ml-4 list-decimal">.*<\/li>\n?)+)/g,
    '<ol class="my-2 space-y-1">$1</ol>'
  )

  // Paragraphs: wrap remaining non-empty lines that aren't already HTML elements
  const lines = html.split('\n')
  const processed = lines.map((line) => {
    const trimmed = line.trim()
    if (!trimmed) return ''
    // Skip lines that are already HTML elements
    if (/^<(h[1-6]|pre|code|ul|ol|li|blockquote|hr|div|p)[\s>]/i.test(trimmed)) {
      return line
    }
    return `<p class="my-1.5 leading-relaxed">${trimmed}</p>`
  })

  html = processed.join('\n')

  return html
}

export function SpecEditor({ specId, content, readOnly = false }: SpecEditorProps) {
  const initialMode: EditorMode = readOnly ? 'preview' : (content ? 'preview' : 'edit')
  const [mode, setMode] = useState<EditorMode>(initialMode)
  const [editedContent, setEditedContent] = useState(content ?? '')
  const updateSpec = useUpdateSpec()

  // Track whether the content has been modified from the original
  const hasChanges = editedContent !== (content ?? '')

  const handleSave = useCallback(async () => {
    if (!hasChanges) return

    try {
      await updateSpec.mutateAsync({
        id: specId,
        input: { final_spec: editedContent },
      })
      toast.success('Spec saved')
      setMode('preview')
    } catch (error) {
      console.error('Failed to save spec:', error)
      toast.error('Failed to save spec', {
        description: error instanceof Error ? error.message : 'An unexpected error occurred',
      })
    }
  }, [specId, editedContent, hasChanges, updateSpec])

  const handleToggleMode = useCallback((newMode: EditorMode) => {
    if (newMode === 'edit' && readOnly) return
    // When switching to edit, ensure we have the latest content
    if (newMode === 'edit') {
      setEditedContent(content ?? '')
    }
    setMode(newMode)
  }, [readOnly, content])

  const renderedHtml = useMemo(() => {
    const textToRender = mode === 'preview' ? (content ?? '') : editedContent
    if (!textToRender) return ''
    return renderMarkdownToHtml(textToRender)
  }, [mode, content, editedContent])

  const isEmpty = !content && !editedContent

  return (
    <Card>
      {/* Toolbar */}
      <CardHeader className="flex flex-row items-center justify-between border-b py-2 px-4">
        <div className="flex items-center gap-1">
          <Button
            variant={mode === 'edit' ? 'default' : 'ghost'}
            size="sm"
            onClick={() => handleToggleMode('edit')}
            disabled={readOnly}
            className="h-7 gap-1.5 px-2.5 text-xs"
          >
            <Pencil className="size-3.5" />
            Edit
          </Button>
          <Button
            variant={mode === 'preview' ? 'default' : 'ghost'}
            size="sm"
            onClick={() => handleToggleMode('preview')}
            className="h-7 gap-1.5 px-2.5 text-xs"
          >
            <Eye className="size-3.5" />
            Preview
          </Button>
        </div>

        {!readOnly && mode === 'edit' && (
          <Button
            size="sm"
            onClick={handleSave}
            disabled={!hasChanges || updateSpec.isPending}
            className="h-7 gap-1.5 px-3 text-xs"
          >
            {updateSpec.isPending ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Save className="size-3.5" />
            )}
            {updateSpec.isPending ? 'Saving...' : 'Save'}
          </Button>
        )}
      </CardHeader>

      <CardContent className="p-0">
        {mode === 'edit' ? (
          <Textarea
            value={editedContent}
            onChange={(e) => setEditedContent(e.target.value)}
            placeholder="Write your spec in markdown..."
            className={cn(
              'min-h-[500px] resize-y rounded-none border-0 font-mono text-sm',
              'focus-visible:ring-0 focus-visible:ring-offset-0',
              'p-4'
            )}
          />
        ) : (
          <div className="min-h-[500px] p-4">
            {isEmpty ? (
              <div className="flex h-[460px] items-center justify-center text-muted-foreground">
                <p className="text-sm">
                  No spec content yet. {!readOnly && 'Switch to Edit mode to start writing.'}
                </p>
              </div>
            ) : (
              <div
                className="prose prose-sm dark:prose-invert max-w-none"
                dangerouslySetInnerHTML={{ __html: renderedHtml }}
              />
            )}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
