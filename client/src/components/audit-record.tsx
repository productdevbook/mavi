import { useLingui } from "@lingui/react/macro"
import { ShieldAlert } from "lucide-react"
import type { AuditEvent } from "@api"

const GRAVE_ACTIONS = ["delete", "trash", "revoke", "replace", "refuse"]

function grave(action: string): boolean {
  return GRAVE_ACTIONS.some((word) => action.includes(word))
}

/**
 * What was done to this site, drawn once.
 *
 * A phrase and how grave it looks belong together: written in two places they
 * drift, and what drifts is which entries somebody's eye stops on.
 */
export function AuditTable({ entries }: { entries: AuditEvent[] }) {
  const { t } = useLingui()

  return (
    <div className="overflow-x-auto rounded-xl border border-border">
      <table className="w-full text-sm">
        <thead className="border-b border-border bg-muted/30 text-left text-xs text-muted-foreground">
          <tr>
            <th className="w-10 py-2 pl-3 font-normal" />
            <th className="py-2 font-normal">{t`Action`}</th>
            <th className="py-2 font-normal">{t`Who`}</th>
            <th className="py-2 pr-3 text-right font-normal">{t`When`}</th>
          </tr>
        </thead>
        <tbody>
          {entries.map((entry) => {
            const isGrave = grave(entry.action)

            return (
              <tr
                key={entry.id}
                className="border-b border-border last:border-0"
              >
                <td className="w-10 py-2 pl-3 align-top">
                  <ShieldAlert
                    className={
                      isGrave
                        ? "size-4 text-destructive"
                        : "size-4 text-muted-foreground"
                    }
                  />
                </td>
                <td className="py-2 align-top">
                  <p className="font-medium">{entry.action}</p>
                  <p className="text-muted-foreground">
                    {entry.resource_type}
                    {entry.resource_id ? ` · ${entry.resource_id}` : ""}
                  </p>
                </td>
                <td className="py-2 align-top">
                  {entry.actor_id ?? entry.actor_kind}
                </td>
                <td className="py-2 pr-3 text-right align-top whitespace-nowrap text-muted-foreground">
                  {new Date(entry.created_at).toLocaleString()}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}
