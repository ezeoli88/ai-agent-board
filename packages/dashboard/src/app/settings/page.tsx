'use client'

import { HelpCircle, Plug } from 'lucide-react'
import { Link } from '@tanstack/react-router'
import { ChromeMcpSection, ConnectionsSection, PreferencesSection, SettingsTour } from '@/features/settings'
import { Button } from '@/components/ui/button'
import { FEATURE_FLAGS } from '@/config/features'

export default function SettingsPage() {
  const handleLaunchTour = () => {
    window.dispatchEvent(new Event('settings-tour-launch'))
  }

  return (
    <div className="space-y-6 animate-in fade-in duration-300">
      {/* Page header */}
      <header className="flex items-start justify-between">
        <div>
          <h1 className="text-2xl md:text-3xl font-bold tracking-tight">Settings</h1>
          <p className="text-sm md:text-base text-muted-foreground">
            Configura las conexiones y preferencias de la aplicacion
          </p>
        </div>
        <Button
          variant="ghost"
          size="icon"
          onClick={handleLaunchTour}
          title="Tour de Settings"
        >
          <HelpCircle className="size-5" />
        </Button>
      </header>

      {/* Settings sections */}
      <div className="space-y-6 max-w-2xl">
        {/* MCP Server */}
        <div className="rounded-lg border border-primary/30 bg-primary/5 p-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Plug className="size-5 text-primary" />
              <div>
                <h3 className="text-sm font-medium">MCP Server</h3>
                <p className="text-xs text-muted-foreground">
                  Ahora puedes conectarte por MCP a Agent Board. Busca tu plataforma favorita y configura la conexion en un click.
                </p>
              </div>
            </div>
            <Button variant="outline" size="sm" asChild>
              <Link to="/mcp-setup">Configurar MCP</Link>
            </Button>
          </div>
        </div>

        <ConnectionsSection id="settings-connections" />
        {FEATURE_FLAGS.qaDashboard && (
          <ChromeMcpSection id="settings-chrome-mcp" />
        )}
        <PreferencesSection id="settings-preferences" />
      </div>

      {/* Interactive tour */}
      <SettingsTour />
    </div>
  )
}
