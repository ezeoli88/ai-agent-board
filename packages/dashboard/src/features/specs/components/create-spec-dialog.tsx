'use client'

import { useState, useEffect } from 'react'
import { FileText, Loader2, Sparkles } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { toast } from 'sonner'
import type { CreateSpecInput } from '../types'
import { useSpecUIStore } from '../stores/spec-ui-store'
import { useCreateSpec } from '../hooks/use-spec-mutations'
import { useRefineSpec } from '../hooks/use-spec-actions'
import { useRepoStore } from '@/features/repos/stores/repo-store'
import { useDetectedAgents } from '@/features/setup/hooks/use-detected-agents'
import { useSettings } from '@/features/setup/hooks/use-settings'
import { getAgentDisplayInfo } from '@/features/tasks/utils/agent-display'

export function CreateSpecDialog() {
  const { isCreateModalOpen, closeCreateModal, openDrawer } = useSpecUIStore()
  const createSpecMutation = useCreateSpec()
  const refineSpecMutation = useRefineSpec()
  const { selectedRepoId } = useRepoStore()
  const { data: agents } = useDetectedAgents()
  const { data: settings } = useSettings()

  const [userInput, setUserInput] = useState('')
  const [agentType, setAgentType] = useState<string>('')
  const [agentModel, setAgentModel] = useState<string>('')

  // Agent derived values
  const installedAgents = (agents ?? []).filter((a) => a.installed)
  const selectedAgentData = installedAgents.find((a) => a.id === agentType)
  const selectedAgentModels = selectedAgentData?.models ?? []

  // Auto-select first installed agent if none selected
  useEffect(() => {
    if (!agentType && installedAgents.length > 0) {
      const defaultId = settings?.default_agent_type ?? installedAgents[0]?.id
      const agent = installedAgents.find((a) => a.id === defaultId) ?? installedAgents[0]
      if (agent) setAgentType(agent.id)
    }
  }, [agentType, installedAgents, settings?.default_agent_type])

  // Auto-select first model when agent changes and current model is not valid
  useEffect(() => {
    if (selectedAgentModels.length > 0 && !selectedAgentModels.find((m) => m.id === agentModel)) {
      setAgentModel(selectedAgentModels[0].id)
    }
  }, [agentType, selectedAgentModels, agentModel])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()

    if (!selectedRepoId) {
      toast.error('No repository selected. Please select a repository first.')
      return
    }

    if (!userInput.trim()) {
      toast.error('Please describe your idea.')
      return
    }

    try {
      const spec = await createSpecMutation.mutateAsync({
        repository_id: selectedRepoId,
        user_input: userInput.trim(),
        title: userInput.trim().slice(0, 100),
        ...(agentType ? { agent_type: agentType as CreateSpecInput['agent_type'] } : {}),
        ...(agentModel ? { agent_model: agentModel } : {}),
      })

      toast.success('Spec created', {
        description: 'The agent will now explore the codebase and generate a spec.',
      })

      closeCreateModal()
      setUserInput('')

      // Open the drawer so the user can follow the agent's progress
      openDrawer(spec.id)

      // Auto-trigger refine to start the agent
      try {
        await refineSpecMutation.mutateAsync(spec.id)
      } catch (refineError) {
        console.error('Failed to start spec refinement:', refineError)
        toast.error('Spec created but failed to start refinement', {
          description:
            refineError instanceof Error
              ? refineError.message
              : 'You can manually trigger refinement from the spec drawer.',
        })
      }
    } catch (error) {
      console.error('Failed to create spec:', error)
      toast.error('Failed to create spec', {
        description: error instanceof Error ? error.message : 'An unexpected error occurred',
      })
    }
  }

  const handleOpenChange = (open: boolean) => {
    if (!open && !createSpecMutation.isPending && !refineSpecMutation.isPending) {
      closeCreateModal()
      setUserInput('')
    }
  }

  const isSubmitting = createSpecMutation.isPending || refineSpecMutation.isPending

  return (
    <Dialog open={isCreateModalOpen} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-[500px] overflow-hidden">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FileText className="h-5 w-5 text-primary" />
            New Spec
          </DialogTitle>
          <DialogDescription>
            Describe your idea. The agent will explore the codebase and generate a technical specification.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-6 pt-2">
          {/* User Input */}
          <div className="space-y-2">
            <Label htmlFor="spec-user-input">What do you want to build?</Label>
            <Textarea
              id="spec-user-input"
              value={userInput}
              onChange={(e) => setUserInput(e.target.value)}
              placeholder="e.g., Add a notification system with email and in-app channels"
              className="min-h-[120px] resize-none"
              disabled={isSubmitting}
            />
            <p className="text-xs text-muted-foreground">
              Describe your idea at a high level. The agent will read the codebase and produce a
              detailed technical spec that you can review and refine.
            </p>
          </div>

          {/* Agent & Model */}
          {installedAgents.length > 0 && (
            <div className="flex gap-3">
              <div className="flex-1 space-y-1.5">
                <Label htmlFor="spec-agent-select" className="text-sm">
                  Agent
                </Label>
                <Select
                  value={agentType}
                  onValueChange={(val) => {
                    setAgentType(val)
                    setAgentModel('')
                  }}
                  disabled={isSubmitting}
                >
                  <SelectTrigger id="spec-agent-select" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {installedAgents.map((agent) => {
                      const info = getAgentDisplayInfo(agent.id)
                      return (
                        <SelectItem key={agent.id} value={agent.id}>
                          {info?.icon} {info?.name ?? agent.name}
                        </SelectItem>
                      )
                    })}
                  </SelectContent>
                </Select>
              </div>

              {selectedAgentModels.length > 0 && (
                <div className="flex-1 space-y-1.5">
                  <Label htmlFor="spec-model-select" className="text-sm">
                    Model
                  </Label>
                  <Select
                    value={agentModel}
                    onValueChange={setAgentModel}
                    disabled={isSubmitting}
                  >
                    <SelectTrigger id="spec-model-select" className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {selectedAgentModels.map((model) => (
                        <SelectItem key={model.id} value={model.id}>
                          {model.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
            </div>
          )}

          {/* Actions */}
          <div className="flex justify-end gap-3">
            <Button
              type="button"
              variant="ghost"
              onClick={() => handleOpenChange(false)}
              disabled={isSubmitting}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={!selectedRepoId || !userInput.trim() || isSubmitting}
            >
              {isSubmitting ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Creating...
                </>
              ) : (
                <>
                  <Sparkles className="h-4 w-4 mr-2" />
                  Generate Spec
                </>
              )}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
