import { useLingui } from "@lingui/react/macro"

import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"

const AGENCY_PLANS = [
  {
    name: "Project",
    price: 79,
    sites: 1,
    popular: false,
    description: "For agencies that launch a few sites as one-off projects.",
  },
  {
    name: "Starter",
    price: 299,
    sites: 5,
    popular: false,
    description: "For a small agency getting its first client sites online.",
  },
  {
    name: "Growth",
    price: 599,
    sites: 20,
    description: "For agencies running a growing portfolio of sites.",
    popular: true,
  },
  {
    name: "Pro",
    price: 1299,
    sites: 75,
    popular: false,
    description: "For high-volume agencies that need room to scale.",
  },
] as const

function planDescription(
  t: ReturnType<typeof useLingui>["t"],
  name: (typeof AGENCY_PLANS)[number]["name"]
) {
  if (name === "Starter") {
    return t`For a small agency getting its first client sites online.`
  }
  if (name === "Project") {
    return t`For agencies that launch a few sites as one-off projects.`
  }
  if (name === "Growth") {
    return t`For agencies running a growing portfolio of sites.`
  }
  return t`For high-volume agencies that need room to scale.`
}

export function AgencyPricingPlans({
  currentPlanId,
  onSelect,
  busy,
}: {
  currentPlanId?: string
  onSelect?: (planId: string) => void
  busy?: boolean
}) {
  const { t } = useLingui()

  return (
    <section className="flex flex-col gap-3">
      <div>
        <h2 className="text-sm font-semibold">{t`Agency plans`}</h2>
        <p className="text-xs text-muted-foreground">
          {t`These fees are charged to your agency balance. You decide what each site owner pays on top.`}
        </p>
      </div>
      <div className="grid gap-3 lg:grid-cols-3">
        {AGENCY_PLANS.map((plan) => (
          <Card key={plan.name} className={plan.popular === true ? "border-primary" : undefined}>
            <CardHeader className="gap-2 pb-3">
              <div className="flex items-center justify-between gap-2">
                <CardTitle className="text-base">{plan.name}</CardTitle>
                {plan.popular === true && <Badge>{t`Recommended`}</Badge>}
              </div>
              <p className="text-2xl font-semibold">
                ${plan.price.toLocaleString()}
                <span className="text-sm font-normal text-muted-foreground">{t` / month`}</span>
              </p>
            </CardHeader>
            <CardContent className="flex flex-col gap-2 text-sm">
              <p className="font-medium">{t`${plan.sites} sites included`}</p>
              <p className="text-xs text-muted-foreground">
                {planDescription(t, plan.name)}
              </p>
              {onSelect && (
                <Button
                  className="mt-2"
                  size="sm"
                  variant={currentPlanId === plan.name.toLowerCase() ? "secondary" : "outline"}
                  disabled={busy || currentPlanId === plan.name.toLowerCase()}
                  onClick={() => onSelect(plan.name.toLowerCase())}
                >
                  {currentPlanId === plan.name.toLowerCase() ? t`Current plan` : t`Choose plan`}
                </Button>
              )}
            </CardContent>
          </Card>
        ))}
      </div>
      <div className="flex flex-col gap-3 rounded-xl border border-border bg-muted/20 p-4">
        <div>
          <h3 className="text-sm font-semibold">{t`How billing works`}</h3>
          <p className="mt-1 text-xs text-muted-foreground">
            {t`There are two sides to the bill: the platform charges your agency, and your agency can pass those costs to each site owner with its own margin.`}
          </p>
        </div>
        <div className="grid gap-3 md:grid-cols-2">
          <div className="rounded-lg border border-border bg-background p-3">
            <p className="text-sm font-medium">{t`1. Your agency plan`}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t`The monthly plan fee is taken from your agency balance. It pays for your agency account and the number of active site slots shown above. It is not automatically a separate charge to a site owner.`}
            </p>
          </div>
          <div className="rounded-lg border border-border bg-background p-3">
            <p className="text-sm font-medium">{t`2. When you launch a site`}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t`Every new site has a one-time $199 activation fee. This is charged once when the site is created; it is not a second monthly plan fee.`}
            </p>
          </div>
          <div className="rounded-lg border border-border bg-background p-3">
            <p className="text-sm font-medium">{t`3. Active site costs`}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t`An active site can receive its own monthly hosting, email and storage charges. Your agency may add a margin before showing those charges to the site owner.`}
            </p>
          </div>
          <div className="rounded-lg border border-border bg-background p-3">
            <p className="text-sm font-medium">{t`4. If a site is archived`}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t`Archiving stops the public site and active usage charges, but keeps its data and media stored. Storage costs $9/month until you restore or delete the site.`}
            </p>
          </div>
        </div>
        <p className="text-xs text-muted-foreground">
          {t`Additional active sites cost $29/month when your plan's included slots are full. The $199 activation fee still applies to each newly created site.`}
        </p>
      </div>
    </section>
  )
}
