import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Globe, Loader2 } from "lucide-react"
import { toast } from "sonner"

import { api } from "@/lib/v1"
import { said } from "@/lib/v1-said"
import { Badge } from "@/components/ui/badge"

/** One address a site answers on, and what was found when it was asked. */
interface Address {
  host: string
  is_primary: boolean
  resolves?: boolean | null
  answered?: boolean | null
  note?: string | null
  checked_at?: string | null
}

/**
 * Whether this site's addresses actually reach it.
 *
 * Asked of the address rather than of the database: a domain that points
 * somewhere else, or a certificate that never arrived, looks exactly like a
 * working site from in here — which is why the first anybody hears of it is a
 * customer saying the site is down.
 */
export function AddressHealth() {
  const { t } = useLingui()
  const [addresses, setAddresses] = React.useState<Address[] | null>(null)

  React.useEffect(() => {
    api("GET /api/domains")
      .then(setAddresses)
      .catch((why: unknown) => {
        toast.error(said(why))
        setAddresses([])
      })
  }, [])

  if (addresses === null) {
    return (
      <div className="flex justify-center py-6">
        <Loader2 className="size-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (addresses.length === 0) {
    return null
  }

  return (
    <section className="flex flex-col gap-3 rounded-xl border border-border p-4">
      <div className="flex items-center gap-2">
        <Globe className="size-4 text-muted-foreground" />
        <h2 className="text-sm font-medium">{t`Addresses`}</h2>
      </div>

      <div className="flex flex-col divide-y divide-border">
        {addresses.map((address) => (
          <div key={address.host} className="flex items-center gap-3 py-2">
            <span className="min-w-0 flex-1 truncate font-mono text-sm">
              {address.host}
            </span>

            {address.is_primary && (
              <Badge variant="secondary">{t`The main one`}</Badge>
            )}

            <Badge variant={address.answered ? "default" : "secondary"}>
              {address.answered
                ? t`Answering`
                : address.resolves
                  ? t`Points here, not answering`
                  : t`Does not point here`}
            </Badge>
          </div>
        ))}
      </div>

      {addresses.some((address) => address.note) && (
        <p className="text-xs text-muted-foreground">
          {addresses.find((address) => address.note)?.note}
        </p>
      )}
    </section>
  )
}
