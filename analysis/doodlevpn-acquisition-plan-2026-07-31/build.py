import ast
import contextlib
import io
import json
from pathlib import Path

import pandas as pd


ROOT = Path(__file__).parent
GENERATED_AT = "2026-07-31T12:00:00+03:00"


WEIGHTS = {
    "evidence_fit": 0.25,
    "profit_potential": 0.20,
    "zero_upfront_cash": 0.15,
    "speed": 0.15,
    "scale": 0.15,
    "measurement": 0.10,
}


CHANNELS = [
    {
        "channel": "Partner Engine: old partners + microauthors CPA",
        "evidence_fit": 95,
        "profit_potential": 90,
        "zero_upfront_cash": 95,
        "speed": 85,
        "scale": 85,
        "measurement": 95,
        "payers_30d_low": 20,
        "payers_30d_base": 100,
        "payers_30d_high": 250,
        "verdict": "Primary motion",
    },
    {
        "channel": "30-day gift-pass referral sprint",
        "evidence_fit": 90,
        "profit_potential": 65,
        "zero_upfront_cash": 90,
        "speed": 90,
        "scale": 60,
        "measurement": 90,
        "payers_30d_low": 15,
        "payers_30d_base": 35,
        "payers_30d_high": 60,
        "verdict": "Second experiment, not the source of new graphs",
    },
    {
        "channel": "Cross-bot and channel rev-share integrations",
        "evidence_fit": 65,
        "profit_potential": 70,
        "zero_upfront_cash": 90,
        "speed": 70,
        "scale": 80,
        "measurement": 80,
        "payers_30d_low": 10,
        "payers_30d_base": 30,
        "payers_30d_high": 80,
        "verdict": "Fold into Partner Engine after first proof",
    },
    {
        "channel": "Family/group plan as a product loop",
        "evidence_fit": 75,
        "profit_potential": 55,
        "zero_upfront_cash": 90,
        "speed": 65,
        "scale": 65,
        "measurement": 85,
        "payers_30d_low": 10,
        "payers_30d_base": 25,
        "payers_30d_high": 50,
        "verdict": "Good product loop; not first acquisition motion",
    },
    {
        "channel": "B2B2C bundles with tech/service communities",
        "evidence_fit": 60,
        "profit_potential": 70,
        "zero_upfront_cash": 90,
        "speed": 55,
        "scale": 75,
        "measurement": 75,
        "payers_30d_low": 5,
        "payers_30d_base": 20,
        "payers_30d_high": 50,
        "verdict": "Explore after the direct partner playbook works",
    },
    {
        "channel": "CPA/affiliate marketplace",
        "evidence_fit": 55,
        "profit_potential": 65,
        "zero_upfront_cash": 75,
        "speed": 55,
        "scale": 80,
        "measurement": 90,
        "payers_30d_low": 10,
        "payers_30d_base": 35,
        "payers_30d_high": 100,
        "verdict": "Potential scale, but weaker control and likely platform fees",
    },
    {
        "channel": "UGC, reviews and customer ambassadors",
        "evidence_fit": 60,
        "profit_potential": 50,
        "zero_upfront_cash": 90,
        "speed": 60,
        "scale": 60,
        "measurement": 65,
        "payers_30d_low": 5,
        "payers_30d_base": 15,
        "payers_30d_high": 40,
        "verdict": "Supporting trust layer, not enough as the primary engine",
    },
    {
        "channel": "Organic setup content and search",
        "evidence_fit": 55,
        "profit_potential": 55,
        "zero_upfront_cash": 95,
        "speed": 30,
        "scale": 85,
        "measurement": 70,
        "payers_30d_low": 0,
        "payers_30d_base": 5,
        "payers_30d_high": 30,
        "verdict": "Long-term compounding channel; too slow for the current hole",
    },
    {
        "channel": "Telegram Mini App + Stars affiliate discovery",
        "evidence_fit": 40,
        "profit_potential": 55,
        "zero_upfront_cash": 80,
        "speed": 30,
        "scale": 85,
        "measurement": 90,
        "payers_30d_low": 0,
        "payers_30d_base": 10,
        "payers_30d_high": 50,
        "verdict": "Future distribution option; requires Stars economics",
    },
    {
        "channel": "Viral Mini App/game season",
        "evidence_fit": 40,
        "profit_potential": 45,
        "zero_upfront_cash": 80,
        "speed": 40,
        "scale": 80,
        "measurement": 75,
        "payers_30d_low": 0,
        "payers_30d_base": 15,
        "payers_30d_high": 80,
        "verdict": "High variance; game has no audience before distribution",
    },
    {
        "channel": "App-store/ASO and bot directories",
        "evidence_fit": 45,
        "profit_potential": 45,
        "zero_upfront_cash": 85,
        "speed": 35,
        "scale": 70,
        "measurement": 65,
        "payers_30d_low": 0,
        "payers_30d_base": 5,
        "payers_30d_high": 20,
        "verdict": "Hygiene, not a growth engine",
    },
    {
        "channel": "Broad paid Telegram placements",
        "evidence_fit": 45,
        "profit_potential": 40,
        "zero_upfront_cash": 10,
        "speed": 75,
        "scale": 70,
        "measurement": 80,
        "payers_30d_low": 0,
        "payers_30d_base": 0,
        "payers_30d_high": 0,
        "verdict": "Rejected: no budget and rising placement prices",
    },
]


channels = pd.DataFrame(CHANNELS)
channels["score"] = sum(channels[field] * weight for field, weight in WEIGHTS.items())
channels = channels.sort_values("score", ascending=False).reset_index(drop=True)
channels.insert(0, "rank", channels.index + 1)
channels["payers_30d_range"] = channels.apply(
    lambda row: f"{int(row.payers_30d_low)}–{int(row.payers_30d_high)} (base {int(row.payers_30d_base)})", axis=1
)


ECON_SCENARIOS = [
    {
        "scenario": "Pessimistic",
        "new_payers": 20,
        "revenue_per_payer_30d": 5.76,
        "first_payment_net": 4.98,
        "first_payment_partner_rate": 0.60,
        "server_cost_per_user": 0.24,
        "gift_days": 7,
        "gift_opportunity_cost_per_payer": 0.73,
        "fraud_reserve_rate": 0.08,
        "month2_retention": 0.45,
        "month3_retention": 0.30,
    },
    {
        "scenario": "Base",
        "new_payers": 100,
        "revenue_per_payer_30d": 5.76,
        "first_payment_net": 4.98,
        "first_payment_partner_rate": 0.60,
        "server_cost_per_user": 0.12,
        "gift_days": 7,
        "gift_opportunity_cost_per_payer": 0.22,
        "fraud_reserve_rate": 0.05,
        "month2_retention": 0.60,
        "month3_retention": 0.45,
    },
    {
        "scenario": "Aggressive",
        "new_payers": 250,
        "revenue_per_payer_30d": 5.76,
        "first_payment_net": 4.98,
        "first_payment_partner_rate": 0.60,
        "server_cost_per_user": 0.05,
        "gift_days": 7,
        "gift_opportunity_cost_per_payer": 0.11,
        "fraud_reserve_rate": 0.03,
        "month2_retention": 0.70,
        "month3_retention": 0.55,
    },
]


economics = pd.DataFrame(ECON_SCENARIOS)
economics["partner_payout_per_payer"] = economics.first_payment_net * economics.first_payment_partner_rate
economics["gift_infra_per_payer"] = economics.server_cost_per_user * economics.gift_days / 30
economics["fraud_reserve_per_payer"] = economics.partner_payout_per_payer * economics.fraud_reserve_rate
economics["contribution_per_payer_30d"] = (
    economics.revenue_per_payer_30d
    - economics.partner_payout_per_payer
    - economics.server_cost_per_user
    - economics.gift_infra_per_payer
    - economics.gift_opportunity_cost_per_payer
    - economics.fraud_reserve_per_payer
)
economics["incremental_revenue_30d"] = economics.new_payers * economics.revenue_per_payer_30d
economics["partner_payout_30d"] = economics.new_payers * economics.partner_payout_per_payer
economics["incremental_contribution_30d"] = economics.new_payers * economics.contribution_per_payer_30d
economics["future_contribution_per_payer"] = (
    (economics.month2_retention + economics.month3_retention)
    * (4.80 * 0.80 - economics.server_cost_per_user)
)
economics["incremental_contribution_90d"] = economics.new_payers * (
    economics.contribution_per_payer_30d + economics.future_contribution_per_payer
)


OUTREACH_SCENARIOS = [
    {
        "scenario": "Pessimistic",
        "old_partners_contacted": 10,
        "old_partners_live": 2,
        "paid_per_old_partner": 8,
        "new_prospects_contacted": 100,
        "reply_rate": 0.20,
        "onboard_rate_of_replies": 0.40,
        "publish_rate_of_onboarded": 0.50,
        "registrations_per_live_new_partner": 25,
        "registration_to_paid": 0.04,
    },
    {
        "scenario": "Base",
        "old_partners_contacted": 10,
        "old_partners_live": 4,
        "paid_per_old_partner": 10,
        "new_prospects_contacted": 200,
        "reply_rate": 0.30,
        "onboard_rate_of_replies": 0.40,
        "publish_rate_of_onboarded": 0.60,
        "registrations_per_live_new_partner": 50,
        "registration_to_paid": 0.08,
    },
    {
        "scenario": "Aggressive",
        "old_partners_contacted": 10,
        "old_partners_live": 6,
        "paid_per_old_partner": 15,
        "new_prospects_contacted": 300,
        "reply_rate": 0.35,
        "onboard_rate_of_replies": 0.45,
        "publish_rate_of_onboarded": 0.70,
        "registrations_per_live_new_partner": 70,
        "registration_to_paid": 0.07,
    },
]


outreach = pd.DataFrame(OUTREACH_SCENARIOS)
outreach["replies"] = outreach.new_prospects_contacted * outreach.reply_rate
outreach["new_partners_onboarded"] = outreach.replies * outreach.onboard_rate_of_replies
outreach["new_partners_live"] = outreach.new_partners_onboarded * outreach.publish_rate_of_onboarded
outreach["new_partner_registrations"] = outreach.new_partners_live * outreach.registrations_per_live_new_partner
outreach["paid_from_new_partners"] = outreach.new_partner_registrations * outreach.registration_to_paid
outreach["paid_from_old_partners"] = outreach.old_partners_live * outreach.paid_per_old_partner
outreach["total_new_payers"] = outreach.paid_from_new_partners + outreach.paid_from_old_partners


channels.to_csv(ROOT / "channel_scoring.csv", index=False)
economics.to_csv(ROOT / "partner_economics.csv", index=False)
outreach.to_csv(ROOT / "outreach_funnel.csv", index=False)


def md(source):
    return {"cell_type": "markdown", "metadata": {}, "source": source.strip()}


def code(source):
    return {
        "cell_type": "code",
        "execution_count": None,
        "metadata": {},
        "outputs": [],
        "source": source.strip(),
    }


nb = {
    "nbformat": 4,
    "nbformat_minor": 5,
    "metadata": {
        "kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"},
        "language_info": {"name": "python", "version": "3"},
    },
}
nb["cells"] = [
    md(
        """# DoodleVPN acquisition plan — 31 July 2026

## tl;dr

The highest-probability zero-upfront acquisition motion is a **Partner Engine**: reactivate proven partners, graduate proven retail referrers into campaign-based ambassadors, and recruit narrowly matched microauthors on performance-only terms. The model recommends **60% of the first net payment + 20% of the next two successful payments**, with a 14-day hold and a seven-day product gift to the referred payer after the first payment.

The model is a sensitivity analysis, not a forecast. Internal facts are from the validated 13 April–30 July DoodleVPN analysis; external mechanics and market evidence are listed in the companion report."""
    ),
    md(
        """## Context & Methods

### Key Assumptions

- 30-day net revenue per special-partner payer: **$5.76**.
- Average first payment: **$4.98**.
- Fully allocated server cost is about **$0.24 per active user-month**; base and aggressive cases use lower marginal cost.
- Channel scores are decision priors, not measured causal performance.
- New-user ranges are scenario assumptions used to compare operational upside.
- The legal environment affects partner supply and execution risk, but is not used as a veto in the ranking."""
    ),
    code(
        """from pathlib import Path
import pandas as pd

ROOT = Path('analysis/doodlevpn-acquisition-plan-2026-07-31')
channels = pd.read_csv(ROOT / 'channel_scoring.csv')
economics = pd.read_csv(ROOT / 'partner_economics.csv')
outreach = pd.read_csv(ROOT / 'outreach_funnel.csv')
pd.set_option('display.max_columns', 100)
pd.set_option('display.float_format', lambda x: f'{x:,.2f}')"""
    ),
    md("## Data"),
    code(
        """channels[['rank', 'channel', 'score', 'payers_30d_range', 'verdict']]"""
    ),
    code(
        """economics[['scenario', 'new_payers', 'incremental_revenue_30d', 'partner_payout_30d', 'incremental_contribution_30d', 'incremental_contribution_90d']]"""
    ),
    code(
        """outreach[['scenario', 'new_prospects_contacted', 'new_partners_live', 'new_partner_registrations', 'paid_from_old_partners', 'paid_from_new_partners', 'total_new_payers']]"""
    ),
    md("## Results"),
    code(
        """winner = channels.iloc[0]
base_econ = economics.query("scenario == 'Base'").iloc[0]
base_funnel = outreach.query("scenario == 'Base'").iloc[0]

assert winner['channel'].startswith('Partner Engine')
assert base_econ['incremental_contribution_30d'] > 0
assert 90 <= base_funnel['total_new_payers'] <= 110

{
    'winner': winner['channel'],
    'score': round(winner['score'], 1),
    'base_30d_revenue': round(base_econ['incremental_revenue_30d'], 2),
    'base_30d_contribution': round(base_econ['incremental_contribution_30d'], 2),
    'base_90d_contribution': round(base_econ['incremental_contribution_90d'], 2),
    'base_modeled_payers': round(base_funnel['total_new_payers']),
}"""
    ),
    md(
        """## Takeaways

1. UX and referral cleanup are conversion multipliers, not an external source of users.
2. The missing source is other people's social graphs. Performance-only micro-partners are the only fast option that requires no upfront media budget and already has a Doodle-specific proof point.
3. A game should not be launched as the acquisition engine: it has no audience until another channel distributes it.
4. The campaign should be killed or redesigned if 500 attributed partner registrations produce below 3% first-paid conversion or negative 30-day contribution.
5. The base case is operationally demanding: roughly 200 qualified outreach attempts, 4 reactivated old partners and about 14 new partners actually publishing."""
    ),
]


def execute_notebook(notebook):
    namespace = {}
    execution_count = 0
    for cell in notebook["cells"]:
        if cell["cell_type"] != "code":
            continue
        execution_count += 1
        cell["execution_count"] = execution_count
        tree = ast.parse(cell["source"], filename=f"cell-{execution_count}")
        stdout = io.StringIO()
        result = None
        with contextlib.redirect_stdout(stdout):
            if tree.body and isinstance(tree.body[-1], ast.Expr):
                prefix = ast.Module(body=tree.body[:-1], type_ignores=[])
                if prefix.body:
                    exec(compile(prefix, f"cell-{execution_count}", "exec"), namespace)
                result = eval(
                    compile(ast.Expression(tree.body[-1].value), f"cell-{execution_count}", "eval"),
                    namespace,
                )
            else:
                exec(compile(tree, f"cell-{execution_count}", "exec"), namespace)
        outputs = []
        printed = stdout.getvalue()
        if printed:
            outputs.append({"output_type": "stream", "name": "stdout", "text": printed})
        if result is not None:
            data = {"text/plain": repr(result)}
            if hasattr(result, "to_html"):
                data["text/html"] = result.to_html()
            outputs.append(
                {
                    "output_type": "execute_result",
                    "execution_count": execution_count,
                    "metadata": {},
                    "data": data,
                }
            )
        cell["outputs"] = outputs


execute_notebook(nb)
(ROOT / "doodlevpn_acquisition_plan.ipynb").write_text(
    json.dumps(nb, ensure_ascii=False, indent=2), encoding="utf-8"
)


def sql_literal(value):
    if value is None or (isinstance(value, float) and pd.isna(value)):
        return "NULL"
    if isinstance(value, (int, float)):
        return str(float(value))
    return "'" + str(value).replace("'", "''") + "'"


def values_sql(name, rows, columns):
    body = ",\n".join(
        "(" + ", ".join(sql_literal(row[column]) for column in columns) + ")" for row in rows
    )
    aliases = ", ".join(columns)
    return f"WITH {name}({aliases}) AS (VALUES\n{body}\n) SELECT * FROM {name};"


channel_rows = channels.to_dict("records")
econ_rows = economics.to_dict("records")
outreach_rows = outreach.to_dict("records")

sources = [
    {
        "id": "internal_growth",
        "label": "DoodleVPN validated growth analysis",
        "path": "analysis/doodlevpn-growth-research-2026-07-30/doodlevpn_growth_research.ipynb",
        "query": {
            "description": "Validated DoodleVPN aggregates from 13 April through 30 July 2026.",
            "engine": "SQLite/Python",
            "tables_used": [
                "accounting.revenue",
                "accounting.revenue_refunds",
                "bot_mirror.users",
                "bot_mirror.payments",
                "bot_mirror.referral_holds",
            ],
            "filters": ["Business flows >= 2026-04-13", "July closed at 2026-07-30 23:59:59 MSK"],
            "metric_definitions": [
                "30-day special-partner payer revenue = $5.76",
                "ordinary referral 7-day paid conversion in mature July cohort ≈35.1%",
                "special-partner 7-day paid conversion in mature July cohort ≈4.7%",
            ],
        },
    },
    {
        "id": "channel_model",
        "label": "DoodleVPN acquisition channel decision model",
        "path": "analysis/doodlevpn-acquisition-plan-2026-07-31/channel_scoring.csv",
        "query": {
            "description": "Weighted comparison of twelve acquisition options; scores and payer ranges are decision assumptions, not forecasts.",
            "engine": "SQLite",
            "sql": values_sql(
                "channel_snapshot",
                channel_rows,
                [
                    "rank",
                    "channel",
                    "score",
                    "evidence_fit",
                    "profit_potential",
                    "zero_upfront_cash",
                    "speed",
                    "scale",
                    "measurement",
                    "payers_30d_low",
                    "payers_30d_base",
                    "payers_30d_high",
                    "verdict",
                ],
            ),
            "metric_definitions": [
                "score = 25% evidence fit + 20% profit potential + 15% zero-upfront cash + 15% speed + 15% scale + 10% measurement",
                "payer ranges are scenario assumptions for the first 30 days",
            ],
        },
    },
    {
        "id": "partner_economics",
        "label": "Partner Engine unit economics",
        "path": "analysis/doodlevpn-acquisition-plan-2026-07-31/partner_economics.csv",
        "query": {
            "description": "Pessimistic, base and aggressive performance-only partner economics.",
            "engine": "SQLite/Python",
            "sql": values_sql(
                "partner_economics_snapshot",
                econ_rows,
                [
                    "scenario",
                    "new_payers",
                    "incremental_revenue_30d",
                    "partner_payout_30d",
                    "incremental_contribution_30d",
                    "incremental_contribution_90d",
                ],
            ),
            "metric_definitions": [
                "partner payout = 60% of average $4.98 first net payment",
                "friend receives seven additional days after first verified payment",
                "30-day contribution subtracts payout, server cost, gift infrastructure, gift opportunity cost and fraud reserve",
                "90-day contribution assumes 20% partner share on the next two successful payments",
            ],
        },
    },
    {
        "id": "outreach_model",
        "label": "Partner recruitment funnel model",
        "path": "analysis/doodlevpn-acquisition-plan-2026-07-31/outreach_funnel.csv",
        "query": {
            "description": "Operational funnel required to produce the modeled payer scenarios.",
            "engine": "SQLite/Python",
            "sql": values_sql(
                "outreach_snapshot",
                outreach_rows,
                [
                    "scenario",
                    "old_partners_live",
                    "new_prospects_contacted",
                    "new_partners_live",
                    "new_partner_registrations",
                    "registration_to_paid",
                    "paid_from_old_partners",
                    "paid_from_new_partners",
                    "total_new_payers",
                ],
            ),
        },
    },
    {"id": "akar_market", "label": "AKAR Russian influencer market study", "href": "https://akarussia.ru/news/novosti-akar/rynok-blogerov-v-rossii-ocenili-v-60-mlrd-rublej/"},
    {"id": "arir_telegram", "label": "ARIR and Telega.in Telegram advertising study", "href": "https://adindex.ru/publication/analitics/search/338484/img/issledovanie_rynka_telegram_reklamy_arir_i_telega_in_2025_1.pdf"},
    {"id": "telegram_inflation", "label": "MTS AdTech Telegram placement price study", "href": "https://adindex.ru/news/researches/2026/02/4/342193.phtml"},
    {"id": "amnezia_affiliate", "label": "Amnezia affiliate terms", "href": "https://amnezia.org/br/partners"},
    {"id": "adguard_affiliate", "label": "AdGuard affiliate terms", "href": "https://adguard.com/ru/partners/affiliate.html"},
    {"id": "telegram_affiliate", "label": "Telegram Affiliate Programs", "href": "https://core.telegram.org/api/bots/referrals"},
    {"id": "telegram_miniapps", "label": "Telegram Mini Apps documentation", "href": "https://core.telegram.org/bots/webapps"},
    {"id": "referral_recipient", "label": "Referral reward recipient research", "href": "https://www.ccsenet.org/journal/index.php/ijbm/article/view/39118"},
    {"id": "reward_uncertainty", "label": "Management Science referral field experiment", "href": "https://pubsonline.informs.org/doi/10.1287/mnsc.2024.05685"},
    {"id": "vpn_ad_law", "label": "Current Russian VPN advertising restriction summary", "href": "https://www.consultant.ru/document/cons_doc_LAW_507825/4cc793b2c6391ed91aec28ce9d0456feba8a8b6b/"},
]

charts = [
    {
        "id": "channel_ranking",
        "title": "Acquisition channel scores",
        "subtitle": "Weighted decision score, 0–100; modeled on DoodleVPN evidence and zero-upfront constraint.",
        "type": "bar",
        "dataset": "channels",
        "sourceId": "channel_model",
        "encodings": {
            "x": {"field": "channel", "type": "nominal", "label": "Channel"},
            "y": {"field": "score", "type": "quantitative", "label": "Score", "format": "number"},
        },
        "valueFormat": "number",
        "options": {"orientation": "horizontal", "valueLabels": True},
    }
]

tables = [
    {
        "id": "channel_table",
        "title": "Twelve acquisition options",
        "subtitle": "First-30-day payer ranges are scenario assumptions, not forecasts.",
        "dataset": "channels",
        "sourceId": "channel_model",
        "defaultSort": {"field": "rank", "direction": "asc"},
        "columns": [
            {"field": "rank", "label": "Rank", "format": "number"},
            {"field": "channel", "label": "Channel"},
            {"field": "score", "label": "Score", "format": "number"},
            {"field": "payers_30d_range", "label": "Modeled first-paid users"},
            {"field": "verdict", "label": "Decision"},
        ],
    },
    {
        "id": "economics_table",
        "title": "Partner Engine economics",
        "subtitle": "Incremental values under 60% first-payment partner share and a seven-day post-payment gift.",
        "dataset": "economics",
        "sourceId": "partner_economics",
        "defaultSort": {"field": "new_payers", "direction": "asc"},
        "columns": [
            {"field": "scenario", "label": "Scenario"},
            {"field": "new_payers", "label": "New payers", "format": "number"},
            {"field": "incremental_revenue_30d", "label": "30d revenue", "format": "currency"},
            {"field": "partner_payout_30d", "label": "Partner payout", "format": "currency"},
            {"field": "incremental_contribution_30d", "label": "30d contribution", "format": "currency"},
            {"field": "incremental_contribution_90d", "label": "90d contribution", "format": "currency"},
        ],
    },
    {
        "id": "outreach_table",
        "title": "Required partner recruitment funnel",
        "subtitle": "The base case requires active outbound work, not a passive affiliate page.",
        "dataset": "outreach",
        "sourceId": "outreach_model",
        "defaultSort": {"field": "new_prospects_contacted", "direction": "asc"},
        "columns": [
            {"field": "scenario", "label": "Scenario"},
            {"field": "old_partners_live", "label": "Old partners live", "format": "number"},
            {"field": "new_prospects_contacted", "label": "New contacts", "format": "number"},
            {"field": "new_partners_live", "label": "New partners live", "format": "number"},
            {"field": "new_partner_registrations", "label": "Registrations", "format": "number"},
            {"field": "registration_to_paid", "label": "Paid conversion", "format": "percent"},
            {"field": "total_new_payers", "label": "New payers", "format": "number"},
        ],
    },
]

blocks = [
    {"id": "title", "type": "markdown", "body": "# DoodleVPN: откуда брать новых пользователей"},
    {
        "id": "executive",
        "type": "markdown",
        "body": """## Executive Summary

- **Одной более понятной рефералки недостаточно.** Она повышает конверсию существующей базы, но не создаёт новые социальные графы.
- **Лучший один acquisition-механизм — Doodle Partner Engine.** Он объединяет возврат десяти просевших партнёров, перевод доказанных рефереров в амбассадоры и системный набор тематических микроавторов с оплатой только после подтверждённой покупки.
- **Предлагаемая экономика:** 60% первой чистой оплаты + 20% следующих двух оплат партнёру; другу — семь дней после первой оплаты; 14-дневный hold, возвраты вычитаются. Базовый сценарий 100 новых плательщиков даёт около $576 новой 30-дневной выручки и примерно $226 contribution после всех модельных затрат.
- **Не строить игру как первый источник трафика.** Без канала распространения игра получает ту же существующую базу и не решает исходную проблему.""",
    },
    {
        "id": "why",
        "type": "markdown",
        "body": """## У Doodle закончился не спрос, а доступ к новым аудиториям

Средний платёж сохранился, а ordinary referral-трафик остаётся качественным. Падение пришло из количества новых регистраций, уменьшения активных рефереров и просадки нескольких специальных партнёров. Следовательно, нужен не ещё один экран внутри бота, а повторяемый способ забирать аудиторию из чужих сообществ.

Российский influencer-рынок уже использует Telegram как крупную площадку, а исследования АРИР/Telega.in отдельно отмечают микро- и нано-каналы как формат точечного измеримого охвата. При этом цены обычных размещений росли быстрее охватов. Это усиливает выбор **performance-only**, а не покупку постов заранее. [АКАР](https://akarussia.ru/news/novosti-akar/rynok-blogerov-v-rossii-ocenili-v-60-mlrd-rublej/), [АРИР/Telega.in](https://adindex.ru/publication/analitics/search/338484/img/issledovanie_rynka_telegram_reklamy_arir_i_telega_in_2025_1.pdf), [MTS AdTech](https://adindex.ru/news/researches/2026/02/4/342193.phtml).""",
    },
    {"id": "ranking_chart", "type": "chart", "chartId": "channel_ranking"},
    {"id": "ranking_table", "type": "table", "tableId": "channel_table"},
    {
        "id": "winner",
        "type": "markdown",
        "body": """## Победитель — не просто партнёрка, а управляемая партнёрская машина

Три источника партнёров запускаются одной системой:

1. **Десять просевших специальных партнёров:** персонально выяснить, почему перестали приводить людей, и вернуть их в 30-дневный спринт без ухудшения старых договорённостей.
2. **Доказанные обычные рефереры:** пригласить пользователей, уже приводивших платящих друзей, в отдельную campaign-based ambassador программу для новых referrals.
3. **Новые микроавторы:** каналы и сообщества вокруг AI-сервисов, устройств и настройки, путешествий, remote work, видео, gaming, Smart TV/роутеров и цифровой приватности.

Это один acquisition engine: единые ссылки, attribution, payout, creatives, anti-fraud и weekly scorecard.""",
    },
    {
        "id": "offer",
        "type": "markdown",
        "body": """## Оффер должен мотивировать автора и не убивать маржу

**Новые партнёры:** 60% первой чистой оплаты и 20% следующих двух успешных оплат. После 20 новых плательщиков за 30 дней ставку первой покупки можно поднять до 70%. Не давать lifetime 50–60% всем новым партнёрам.

**Новый пользователь:** обычный трёхдневный trial плюс семь дней после первой подтверждённой оплаты. Это конкретная выгода получателю, а не обещание заработка отправителю. Исследования referral-design показывают, что для low-involvement-продуктов выгода получателю способна улучшать принятие рекомендации; крупный field experiment также обнаружил преимущество определённой награды получателю. [Recipient reward research](https://www.ccsenet.org/journal/index.php/ijbm/article/view/39118), [Management Science field experiment](https://pubsonline.informs.org/doi/10.1287/mnsc.2024.05685).

Условия остаются конкурентными: Amnezia публично предлагает до 40% продажи и 30% продлений, AdGuard — до 50% продаж и продлений. Doodle компенсирует меньший бренд более высокой front-loaded ставкой, но ограничивает recurring. [Amnezia](https://amnezia.org/br/partners), [AdGuard](https://adguard.com/ru/partners/affiliate.html).""",
    },
    {"id": "economics", "type": "table", "tableId": "economics_table"},
    {
        "id": "funnel",
        "type": "markdown",
        "body": """## Базовый сценарий требует около 200 качественных контактов

Пассивная страница «станьте партнёром» не сработает. Базовая модель предполагает: вернуть четыре старых партнёра, сделать 200 персональных выходов на новых авторов, получить около 60 ответов, подключить 24 и добиться фактического размещения примерно от 14. Вместе это моделирует около 100 новых плательщиков. Это цель для операционной команды, а не обещание.""",
    },
    {"id": "outreach", "type": "table", "tableId": "outreach_table"},
    {
        "id": "steps",
        "type": "markdown",
        "body": """## План запуска на 30 дней

### Дни 1–2: измерение

- Заполнить рабочие `campaigns`, `campaign_links`, attribution и conversion ledger.
- Для каждой ссылки хранить partner, niche, creative, first touch, registration, trial, first connection, checkout, payment, refund и renewal.
- Не считать start или клик успехом.

### Дни 3–4: пакет партнёра

- Утвердить новые условия только для новых referrals.
- Сделать одну партнёрскую страницу, один отчёт и три коротких creative-шаблона под разные use cases.
- Ссылка должна вести в персонализированный onboarding, а не на общий экран бота.

### Дни 5–7: вернуть проверенных

- Связаться со всеми десятью просевшими партнёрами лично.
- Задать три вопроса: почему остановились, что перестало конвертировать, какой формат аудитория примет сейчас.
- Цель: минимум пять содержательных ответов и три повторных запуска.

### Дни 5–14: набрать новую supply

- Собрать 200 авторов размером примерно 2k–100k подписчиков.
- Фильтры: естественная потребность в продукте, живая аудитория, нормальные просмотры, отсутствие giveaway/scam-истории, возможность измерить ссылкой.
- Делать 20 персонализированных контактов в день. Не рассылать один одинаковый спам-текст.

### Дни 8–21: волны по пять партнёров

- Запускать не всех сразу, а волнами по нишам.
- Для каждого фиксировать creative и recipient offer.
- После первых 100 регистраций на партнёра: scale при paid conversion ≥8%; оставить и донабрать данные при 5–8%; остановить конкретный источник при <3%, fraud/refunds >15% или отрицательном contribution.

### Дни 22–30: удвоить победителей

- Оставить две лучшие ниши и два лучших creative.
- Повторно выйти к похожим авторам.
- Не добавлять одновременно игру, новую массовую рефералку и большую скидку: иначе источник lift нельзя будет определить.""",
    },
    {
        "id": "metrics",
        "type": "markdown",
        "body": """## Цифры принятия решения

**Продолжить после 30 дней:** не менее пяти реально публикующих партнёров, не менее 50 дополнительных подтверждённых плательщиков, положительный 30-дневный contribution и refund/fraud ниже 8%.

**Масштабировать:** 100+ дополнительных плательщиков, registration→first-paid не ниже 7%, contribution не ниже $1.50 на нового плательщика, ни один партнёр не даёт более 35% новых оплат.

**Переделать оффер:** после 150 качественных контактов запустились менее пяти партнёров. Это означает слабое предложение авторам или плохой список.

**Остановить канал:** после 500 атрибутированных партнёрских регистраций paid conversion ниже 3% либо contribution отрицательный. Останавливать нужно не после первого неудачного автора, а после достаточного совокупного трафика.""",
    },
    {
        "id": "not_now",
        "type": "markdown",
        "body": """## Что сейчас не делать

- Не строить игру как самостоятельный источник пользователей.
- Не покупать крупные Telegram-посты заранее.
- Не менять массовую рефералку и партнёрскую программу одновременно.
- Не давать выплаты за регистрацию, trial или клик.
- Не выдавать новым партнёрам 50–60% со всех продлений навсегда.
- Не ждать, что Mini App Store сам даст discovery: Telegram пишет лишь о возможности feature для успешных Mini Apps со Stars, а affiliate commissions привязаны к Stars-транзакциям. [Telegram Mini Apps](https://core.telegram.org/bots/webapps), [Telegram Affiliate Programs](https://core.telegram.org/api/bots/referrals).
- Не путать CRM/winback с притоком новых пользователей.
- Не считать UX-аудит acquisition-стратегией: он нужен, но является multiplier.""",
    },
    {
        "id": "legal",
        "type": "markdown",
        "body": """## Регуляторный риск не отменяет математику, но влияет на supply

План выше выбран без превращения закона в автоматический veto. Однако с сентября 2025 года действует запрет и штрафы за рекламу соответствующих средств доступа, причём ответственность указана для рекламодателя и распространителя. Это влияет хотя бы на то, сколько авторов согласится работать и какие форматы они примут. Перед публичными интеграциями нужен review конкретного текста и канала; скрывать рекламу или строить стратегию на предположении «точно никого не тронут» нельзя считать бизнес-контролем. [Текущая правовая сводка](https://www.consultant.ru/document/cons_doc_LAW_507825/4cc793b2c6391ed91aec28ce9d0456feba8a8b6b/).""",
    },
    {
        "id": "questions",
        "type": "markdown",
        "body": """## Further Questions

- Почему конкретно остановился каждый из топ-10 партнёров: аудитория исчерпалась, продукт перестал работать, изменилась экономика или они перестали публиковать?
- Какой partner creative давал качественных плательщиков, а не только регистрации?
- Какая доля специальных партнёрских плательщиков продлевается на второй и третий месяц?
- Поддерживает ли текущий checkout частичную оплату внутренним балансом? Это влияет на последующий массовый referral sprint, но не блокирует Partner Engine.""",
    },
    {
        "id": "caveats",
        "type": "markdown",
        "body": """## Caveats and Assumptions

Канал выбран с высокой относительной, но средней абсолютной уверенностью. В Doodle нет причинного эксперимента по acquisition-каналам, а campaign-таблицы были пустыми. Диапазоны новых плательщиков и weighted scores являются прозрачными priors, не прогнозами. Внутренние агрегаты закрыты на 30 июля 2026 года; июльская специальная конверсия около 4.7% показывает, что простое наращивание старого партнёрского трафика без отбора и персонального onboarding не сработает.""",
    },
]

artifact = {
    "surface": "report",
    "manifest": {
        "version": 1,
        "surface": "report",
        "title": "DoodleVPN: откуда брать новых пользователей",
        "description": "Decision report comparing twelve zero-budget acquisition options and specifying the selected Partner Engine.",
        "generatedAt": GENERATED_AT,
        "sources": sources,
        "charts": charts,
        "tables": tables,
        "blocks": blocks,
    },
    "snapshot": {
        "version": 1,
        "generatedAt": GENERATED_AT,
        "status": "ready",
        "datasets": {
            "channels": channel_rows,
            "economics": econ_rows,
            "outreach": outreach_rows,
        },
    },
    "sources": sources,
}

(ROOT / "artifact.json").write_text(json.dumps(artifact, ensure_ascii=False, indent=2), encoding="utf-8")
print(ROOT / "doodlevpn_acquisition_plan.ipynb")
print(ROOT / "artifact.json")
