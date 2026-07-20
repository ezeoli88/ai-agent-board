'use client'

import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useRouter } from '@tanstack/react-router'
import { toast } from 'sonner'
import { specsApi } from '@/lib/api-client'
import { taskKeys } from '@/features/tasks/hooks/query-keys'
import { specKeys } from './query-keys'

export function useSpecActions(specId: string) {
  const queryClient = useQueryClient()
  const router = useRouter()

  const invalidateSpec = () => {
    queryClient.invalidateQueries({ queryKey: specKeys.detail(specId) })
    queryClient.invalidateQueries({ queryKey: specKeys.lists() })
  }

  const generateSpec = useMutation({
    mutationFn: () => specsApi.generateSpec(specId),
    onSuccess: () => {
      toast.success('Spec draft generated')
      invalidateSpec()
    },
    onError: (error: Error) => toast.error(`Failed to generate spec: ${error.message}`),
  })

  const approveSpec = useMutation({
    mutationFn: (content?: string) => specsApi.approveSpec(specId, content),
    onSuccess: () => {
      toast.success('Spec approved')
      invalidateSpec()
    },
    onError: (error: Error) => toast.error(`Failed to approve spec: ${error.message}`),
  })

  const answerClarifications = useMutation({
    mutationFn: (answers: string[]) => specsApi.answerClarifications(specId, { answers }),
    onSuccess: () => {
      toast.success('Spec draft generated from answers')
      invalidateSpec()
    },
    onError: (error: Error) => toast.error(`Failed to generate spec: ${error.message}`),
  })

  const generatePlan = useMutation({
    mutationFn: () => specsApi.generatePlan(specId),
    onSuccess: () => {
      toast.success('Plan generated')
      invalidateSpec()
    },
    onError: (error: Error) => toast.error(`Failed to generate plan: ${error.message}`),
  })

  const approvePlan = useMutation({
    mutationFn: (content?: string) => specsApi.approvePlan(specId, content),
    onSuccess: () => {
      toast.success('Plan approved')
      invalidateSpec()
    },
    onError: (error: Error) => toast.error(`Failed to approve plan: ${error.message}`),
  })

  const generateTasks = useMutation({
    mutationFn: () => specsApi.generateTasks(specId),
    onSuccess: () => {
      toast.success('Task breakdown generated')
      invalidateSpec()
    },
    onError: (error: Error) => toast.error(`Failed to generate task breakdown: ${error.message}`),
  })

  const approveTasks = useMutation({
    mutationFn: (content?: string) => specsApi.approveTasks(specId, content),
    onSuccess: () => {
      toast.success('Task breakdown approved')
      invalidateSpec()
    },
    onError: (error: Error) => toast.error(`Failed to approve task breakdown: ${error.message}`),
  })

  const createTasks = useMutation({
    mutationFn: () => specsApi.createTasks(specId),
    onSuccess: (data) => {
      toast.success('Tasks created from spec', {
        description: `${data.task_ids.length} task${data.task_ids.length === 1 ? '' : 's'} added to Tasks.`,
      })
      invalidateSpec()
      queryClient.invalidateQueries({ queryKey: taskKeys.lists() })
    },
    onError: (error: Error) => toast.error(`Failed to create tasks: ${error.message}`),
  })

  const deleteSpec = useMutation({
    mutationFn: () => specsApi.delete(specId),
    onSuccess: () => {
      toast.success('Spec deleted')
      queryClient.removeQueries({ queryKey: specKeys.detail(specId) })
      queryClient.invalidateQueries({ queryKey: specKeys.lists() })
      router.navigate({ to: '/specs' })
    },
    onError: (error: Error) => toast.error(`Failed to delete spec: ${error.message}`),
  })

  return {
    generateSpec,
    answerClarifications,
    approveSpec,
    generatePlan,
    approvePlan,
    generateTasks,
    approveTasks,
    createTasks,
    deleteSpec,
  }
}
