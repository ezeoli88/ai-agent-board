'use client'

import { useState, useEffect, useCallback } from 'react'
import { X } from 'lucide-react'
import { Button } from '@/components/ui/button'

const TOUR_STORAGE_KEY = 'settings_tour_completed'

const TOUR_STEPS = [
  {
    targetId: 'settings-connections',
    title: 'Conexiones',
    description: 'Configura tu agente de coding CLI, un proveedor de IA (para generar specs) y tu cuenta de GitHub o GitLab (para crear PRs/MRs). El agente CLI es lo mas importante — es quien modifica tu codigo.',
  },
  {
    targetId: 'settings-preferences',
    title: 'Preferencias',
    description: 'Personaliza el rol activo, el tema (claro/oscuro) y el idioma de las especificaciones generadas.',
  },
]

export function SettingsTour() {
  const [visible, setVisible] = useState(false)
  const [step, setStep] = useState(0)

  useEffect(() => {
    const completed = localStorage.getItem(TOUR_STORAGE_KEY)
    if (!completed) {
      // Small delay so the page renders first
      const timer = setTimeout(() => setVisible(true), 500)
      return () => clearTimeout(timer)
    }
  }, [])

  const highlightStep = useCallback((stepIndex: number) => {
    // Remove highlight from all
    TOUR_STEPS.forEach((s) => {
      const el = document.getElementById(s.targetId)
      if (el) {
        el.classList.remove('ring-2', 'ring-primary', 'ring-offset-2', 'transition-all', 'duration-300')
      }
    })

    // Add highlight to current
    const currentEl = document.getElementById(TOUR_STEPS[stepIndex].targetId)
    if (currentEl) {
      currentEl.classList.add('ring-2', 'ring-primary', 'ring-offset-2', 'transition-all', 'duration-300')
      currentEl.scrollIntoView({ behavior: 'smooth', block: 'center' })
    }
  }, [])

  useEffect(() => {
    if (visible) {
      highlightStep(step)
    }
    return () => {
      // Cleanup highlights on unmount
      TOUR_STEPS.forEach((s) => {
        const el = document.getElementById(s.targetId)
        if (el) {
          el.classList.remove('ring-2', 'ring-primary', 'ring-offset-2', 'transition-all', 'duration-300')
        }
      })
    }
  }, [visible, step, highlightStep])

  const completeTour = useCallback(() => {
    localStorage.setItem(TOUR_STORAGE_KEY, 'true')
    setVisible(false)
    // Remove all highlights
    TOUR_STEPS.forEach((s) => {
      const el = document.getElementById(s.targetId)
      if (el) {
        el.classList.remove('ring-2', 'ring-primary', 'ring-offset-2', 'transition-all', 'duration-300')
      }
    })
  }, [])

  const next = () => {
    if (step < TOUR_STEPS.length - 1) {
      setStep(step + 1)
    } else {
      completeTour()
    }
  }

  const prev = () => {
    if (step > 0) {
      setStep(step - 1)
    }
  }

  // Public method to re-launch
  const launch = useCallback(() => {
    setStep(0)
    setVisible(true)
  }, [])

  // Expose launch via custom event
  useEffect(() => {
    const handler = () => launch()
    window.addEventListener('settings-tour-launch', handler)
    return () => window.removeEventListener('settings-tour-launch', handler)
  }, [launch])

  if (!visible) return null

  const currentStep = TOUR_STEPS[step]
  const isLast = step === TOUR_STEPS.length - 1

  return (
    <div className="fixed bottom-6 right-6 z-50 w-80 rounded-lg border bg-card shadow-lg p-4 animate-in slide-in-from-bottom-4 fade-in duration-300">
      {/* Header with step indicator and close */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <span>Paso {step + 1} de {TOUR_STEPS.length}</span>
          <div className="flex gap-1 ml-1">
            {TOUR_STEPS.map((_, i) => (
              <div
                key={i}
                className={`size-1.5 rounded-full transition-colors ${
                  i === step ? 'bg-primary' : 'bg-muted-foreground/30'
                }`}
              />
            ))}
          </div>
        </div>
        <button
          onClick={completeTour}
          className="text-muted-foreground hover:text-foreground transition-colors"
        >
          <X className="size-4" />
        </button>
      </div>

      {/* Content */}
      <h4 className="font-semibold mb-1">{currentStep.title}</h4>
      <p className="text-sm text-muted-foreground mb-4">{currentStep.description}</p>

      {/* Navigation */}
      <div className="flex items-center justify-between">
        <Button variant="ghost" size="sm" onClick={completeTour}>
          Omitir
        </Button>
        <div className="flex gap-2">
          {step > 0 && (
            <Button variant="outline" size="sm" onClick={prev}>
              Anterior
            </Button>
          )}
          <Button size="sm" onClick={next}>
            {isLast ? 'Entendido' : 'Siguiente'}
          </Button>
        </div>
      </div>
    </div>
  )
}
