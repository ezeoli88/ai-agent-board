'use client'

import { useEffect, useState } from 'react'
import { Loader2, Plus } from 'lucide-react'
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
import { useRolePermissions } from '@/features/settings'
import { useRepoStore } from '@/features/repos'
import { useDetectedAgents } from '@/features/setup/hooks/use-detected-agents'
import { useSettings } from '@/features/setup/hooks/use-settings'
import { getAgentDisplayInfo } from '@/features/tasks/utils/agent-display'
import { useCreateSpec } from '../hooks/use-create-spec'
import { useSpecUIStore } from '../stores/spec-ui-store'
import type { CreateSpecInput } from '../types'

export function CreateSpecDialog() {
  const { canManageSpecs } = useRolePermissions()
  const closeCreateModal = useSpecUIStore((state) => state.closeCreateModal)

  useEffect(() => {
    if (!canManageSpecs) {
      closeCreateModal()
    }
  }, [canManageSpecs, closeCreateModal])

  if (!canManageSpecs) {
    return null
  }

  return <CreateSpecDialogContent />
}

function CreateSpecDialogContent() {
  const { selectedRepoId, selectedRepo } = useRepoStore()
  const { data: agents } = useDetectedAgents()
  const { data: settings } = useSettings()
  const createSpec = useCreateSpec()
  const { isCreateModalOpen, closeCreateModal } = useSpecUIStore()

  const [userInput, setUserInput] = useState('')
  const [agentType, setAgentType] = useState('')
  const [agentModel, setAgentModel] = useState('')

  const installedAgents = (agents ?? []).filter((agent) => agent.installed)
  const selectedAgent = installedAgents.find((agent) => agent.id === agentType)
  const selectedModels = selectedAgent?.models ?? []

  useEffect(() => {
    if (!agentType && installedAgents.length > 0) {
      const defaultId = settings?.default_agent_type ?? installedAgents[0]?.id
      const agent = installedAgents.find((item) => item.id === defaultId) ?? installedAgents[0]
      if (agent) setAgentType(agent.id)
    }
  }, [agentType, installedAgents, settings?.default_agent_type])

  useEffect(() => {
    if (selectedModels.length > 0 && !selectedModels.find((model) => model.id === agentModel)) {
      setAgentModel(selectedModels[0].id)
    }
  }, [agentModel, selectedModels])

  const handleOpenChange = (open: boolean) => {
    if (!open && !createSpec.isPending) {
      closeCreateModal()
      setUserInput('')
    }
  }

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()

    if (!selectedRepoId || !selectedRepo) {
      toast.error('No repository selected. Please select a repository first.')
      return
    }

    try {
      await createSpec.mutateAsync({
        repository_id: selectedRepoId,
        user_input: userInput.trim(),
        title: userInput.trim().slice(0, 100),
        ...(agentType ? { agent_type: agentType as CreateSpecInput['agent_type'] } : {}),
        ...(agentModel ? { agent_model: agentModel } : {}),
      })
      closeCreateModal()
      setUserInput('')
    } catch {
      // Toast is handled by the mutation hook.
    }
  }

  return (
    <Dialog open={isCreateModalOpen} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-[560px] overflow-hidden">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Plus className="h-5 w-5 text-primary" />
            New Spec
          </DialogTitle>
          <DialogDescription>
            Create an SDD idea for the selected repository. The agent will turn it into a spec, plan, and task breakdown.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-6 pt-2">
          <div className="space-y-2">
            <Label htmlFor="spec-input">What should this spec cover?</Label>
            <Textarea
              id="spec-input"
              value={userInput}
              onChange={(event) => setUserInput(event.target.value)}
              placeholder="e.g., Add a separate SDD board where specs move through approval stages"
              className="min-h-[140px] resize-none"
              disabled={createSpec.isPending}
            />
          </div>

          {installedAgents.length > 0 && (
            <div className="flex gap-3">
              <div className="flex-1 space-y-1.5">
                <Label htmlFor="spec-agent-select" className="text-sm">Agent</Label>
                <Select
                  value={agentType}
                  onValueChange={(value) => {
                    setAgentType(value)
                    setAgentModel('')
                  }}
                  disabled={createSpec.isPending}
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

              {selectedModels.length > 0 && (
                <div className="flex-1 space-y-1.5">
                  <Label htmlFor="spec-model-select" className="text-sm">Model</Label>
                  <Select
                    value={agentModel}
                    onValueChange={setAgentModel}
                    disabled={createSpec.isPending}
                  >
                    <SelectTrigger id="spec-model-select" className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {selectedModels.map((model) => (
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

          <div className="flex justify-end gap-3">
            <Button type="button" variant="ghost" onClick={() => handleOpenChange(false)} disabled={createSpec.isPending}>
              Cancel
            </Button>
            <Button type="submit" disabled={!selectedRepoId || !userInput.trim() || createSpec.isPending}>
              {createSpec.isPending ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Creating...
                </>
              ) : (
                <>
                  <Plus className="h-4 w-4" />
                  Create Spec
                </>
              )}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
