'use client'

import { FormEvent, useEffect, useMemo, useState } from 'react'
import { AlertCircle, CheckCircle2, Chrome, Loader2, Save } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/lib/utils'
import { useChromeMcpConfig, useUpdateChromeMcpConfig } from '../hooks/use-chrome-mcp-config'

export function ChromeMcpSection({ id }: { id?: string }) {
  const { data: config, isLoading, error } = useChromeMcpConfig()
  const updateConfig = useUpdateChromeMcpConfig()
  const [enabled, setEnabled] = useState(false)
  const [url, setUrl] = useState('')
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    if (!config) return
    setEnabled(config.enabled)
    setUrl(config.url ?? '')
  }, [config])

  const normalizedUrl = url.trim()
  const savedUrl = config?.url ?? ''
  const isDirty = useMemo(() => {
    if (!config) return false
    return config.enabled !== enabled || savedUrl !== normalizedUrl
  }, [config, enabled, normalizedUrl, savedUrl])
  const canSave = !updateConfig.isPending && (!enabled || normalizedUrl.length > 0) && isDirty
  const errorMessage =
    error instanceof Error
      ? error.message
      : updateConfig.error instanceof Error
        ? updateConfig.error.message
        : null

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!canSave) return

    updateConfig.mutate(
      {
        enabled,
        url: normalizedUrl || null,
      },
      {
        onSuccess: () => {
          setSaved(true)
          window.setTimeout(() => setSaved(false), 1800)
        },
      }
    )
  }

  return (
    <Card id={id}>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-lg">
          <Chrome className="size-5" />
          Chrome MCP
        </CardTitle>
        <CardDescription>
          Habilita Chrome DevTools MCP para Claude Code y configura el endpoint que usara QA.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="space-y-4">
            <Skeleton className="h-5 w-40" />
            <Skeleton className="h-9 w-full" />
            <Skeleton className="h-8 w-28" />
          </div>
        ) : (
          <form className="space-y-4" onSubmit={handleSubmit}>
            <label className="flex items-center gap-3 text-sm font-medium">
              <input
                type="checkbox"
                checked={enabled}
                onChange={(event) => {
                  setEnabled(event.target.checked)
                  setSaved(false)
                }}
                className="size-4 rounded border border-input accent-primary"
              />
              Habilitar Chrome MCP para QA
            </label>

            <div className="space-y-1.5">
              <Label htmlFor="chrome-mcp-url">Endpoint MCP</Label>
              <Input
                id="chrome-mcp-url"
                value={url}
                onChange={(event) => {
                  setUrl(event.target.value)
                  setSaved(false)
                }}
                placeholder="http://localhost:9222/mcp"
                aria-invalid={enabled && normalizedUrl.length === 0}
                className="font-mono text-xs"
              />
              <p className="text-xs text-muted-foreground">
                QA lo recibe como <code className="rounded bg-muted px-1 py-0.5">CHROME_MCP_URL</code>.
                Claude Code podra usar <code className="rounded bg-muted px-1 py-0.5">mcp__chrome-devtools__*</code>{' '}
                si el servidor MCP esta configurado en Claude.
              </p>
            </div>

            {errorMessage ? (
              <div className="flex items-center gap-2 text-sm text-destructive">
                <AlertCircle className="size-4" />
                {errorMessage}
              </div>
            ) : null}

            <div className="flex flex-wrap items-center gap-3">
              <Button type="submit" size="sm" disabled={!canSave}>
                {updateConfig.isPending ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <Save className="size-4" />
                )}
                Guardar
              </Button>
              <div
                className={cn(
                  'flex items-center gap-1 text-xs text-muted-foreground transition-opacity',
                  saved ? 'opacity-100' : 'opacity-0'
                )}
              >
                <CheckCircle2 className="size-3.5 text-green-600" />
                Guardado
              </div>
            </div>
          </form>
        )}
      </CardContent>
    </Card>
  )
}
