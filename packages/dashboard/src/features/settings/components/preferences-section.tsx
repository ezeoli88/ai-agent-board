'use client'

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { RoleSelector } from './role-selector'
import { ThemeSelector } from './theme-selector'

/**
 * Preferences section with theme settings
 */
export function PreferencesSection({ id }: { id?: string }) {
  return (
    <Card id={id}>
      <CardHeader>
        <CardTitle className="text-lg">Preferencias</CardTitle>
        <CardDescription>
          Personaliza tu rol y la apariencia de la aplicacion
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <RoleSelector />
        <ThemeSelector />
      </CardContent>
    </Card>
  )
}
