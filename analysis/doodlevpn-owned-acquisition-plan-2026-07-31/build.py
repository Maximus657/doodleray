import ast
import contextlib
import csv
import io
import json
from pathlib import Path


ROOT = Path(__file__).parent
ROOT.mkdir(parents=True, exist_ok=True)
GENERATED_AT = "2026-07-31T18:00:00+03:00"


WEIGHTS = {
    "cold_pull": 0.25,
    "doodle_fit": 0.20,
    "zero_upfront": 0.15,
    "speed_to_evidence": 0.15,
    "scalability": 0.15,
    "economics_measurement": 0.10,
}


PLAYS = [
    {
        "play": "Doodle Rescue: 1 GB reserve + Network Doctor + problem capture",
        "cold_pull": 95,
        "doodle_fit": 95,
        "zero_upfront": 90,
        "speed_to_evidence": 75,
        "scalability": 90,
        "economics_measurement": 80,
        "verdict": "Primary 90-day engine",
    },
    {
        "play": "Network Doctor + current 3-day trial",
        "cold_pull": 75,
        "doodle_fit": 95,
        "zero_upfront": 95,
        "speed_to_evidence": 90,
        "scalability": 80,
        "economics_measurement": 90,
        "verdict": "Fallback if free reserve fails",
    },
    {
        "play": "Manual expert answers in communities",
        "cold_pull": 72,
        "doodle_fit": 85,
        "zero_upfront": 95,
        "speed_to_evidence": 80,
        "scalability": 55,
        "economics_measurement": 65,
        "verdict": "Distribution layer for Rescue",
    },
    {
        "play": "Outsider micro-affiliates on verified revenue only",
        "cold_pull": 68,
        "doodle_fit": 65,
        "zero_upfront": 90,
        "speed_to_evidence": 70,
        "scalability": 80,
        "economics_measurement": 90,
        "verdict": "Acceleration after unit economics proof",
    },
    {
        "play": "Long-tail problem content with current offer",
        "cold_pull": 68,
        "doodle_fit": 75,
        "zero_upfront": 95,
        "speed_to_evidence": 45,
        "scalability": 90,
        "economics_measurement": 80,
        "verdict": "Necessary compounding layer, weak alone",
    },
    {
        "play": "Free browser extension -> paid full-device VPN",
        "cold_pull": 75,
        "doodle_fit": 55,
        "zero_upfront": 70,
        "speed_to_evidence": 45,
        "scalability": 85,
        "economics_measurement": 80,
        "verdict": "Good later wedge, too much new product now",
    },
    {
        "play": "Family/group plan and gifting",
        "cold_pull": 45,
        "doodle_fit": 80,
        "zero_upfront": 90,
        "speed_to_evidence": 70,
        "scalability": 55,
        "economics_measurement": 90,
        "verdict": "Monetization loop, not cold acquisition",
    },
    {
        "play": "App-store listings and ASO",
        "cold_pull": 55,
        "doodle_fit": 75,
        "zero_upfront": 85,
        "speed_to_evidence": 45,
        "scalability": 75,
        "economics_measurement": 70,
        "verdict": "Distribution hygiene",
    },
    {
        "play": "B2B2C bundles with services and communities",
        "cold_pull": 60,
        "doodle_fit": 70,
        "zero_upfront": 85,
        "speed_to_evidence": 50,
        "scalability": 75,
        "economics_measurement": 75,
        "verdict": "Secondary portfolio channel",
    },
    {
        "play": "Referral UX/sprint for the current base",
        "cold_pull": 35,
        "doodle_fit": 90,
        "zero_upfront": 95,
        "speed_to_evidence": 90,
        "scalability": 40,
        "economics_measurement": 90,
        "verdict": "Multiplier only; social graphs are saturated",
    },
    {
        "play": "Telegram Mini App game",
        "cold_pull": 45,
        "doodle_fit": 50,
        "zero_upfront": 80,
        "speed_to_evidence": 55,
        "scalability": 70,
        "economics_measurement": 65,
        "verdict": "No distribution of its own",
    },
    {
        "play": "Telegram Stars affiliate discovery",
        "cold_pull": 50,
        "doodle_fit": 40,
        "zero_upfront": 75,
        "speed_to_evidence": 50,
        "scalability": 75,
        "economics_measurement": 85,
        "verdict": "Requires Stars purchase path and separate economics",
    },
    {
        "play": "Broad paid placements",
        "cold_pull": 85,
        "doodle_fit": 70,
        "zero_upfront": 0,
        "speed_to_evidence": 95,
        "scalability": 90,
        "economics_measurement": 90,
        "verdict": "Infeasible under the zero-budget constraint",
    },
]


for play in PLAYS:
    play["score"] = round(sum(play[key] * weight for key, weight in WEIGHTS.items()), 1)
PLAYS.sort(key=lambda row: row["score"], reverse=True)
for index, play in enumerate(PLAYS, 1):
    play["rank"] = index


ECONOMICS = []
for scenario, conversion_rate in [("Kill boundary", 0.03), ("Base", 0.06), ("Scale", 0.08)]:
    activated = 100
    revenue_per_payer = 6.03
    cost_per_active = 0.24
    payers = activated * conversion_rate
    revenue = payers * revenue_per_payer
    infrastructure = activated * cost_per_active
    contribution = revenue - infrastructure
    ECONOMICS.append(
        {
            "scenario": scenario,
            "activated_free_users": activated,
            "paid_conversion": conversion_rate,
            "new_payers": payers,
            "revenue_30d": round(revenue, 2),
            "server_cost_30d": round(infrastructure, 2),
            "contribution_before_support": round(contribution, 2),
        }
    )


GAP = {
    "may_revenue": 3581.06,
    "july_revenue": 2823.94,
    "monthly_gap": round(3581.06 - 2823.94, 2),
    "revenue_per_new_payer_30d": 6.03,
}
GAP["payers_to_close_gap"] = round(GAP["monthly_gap"] / GAP["revenue_per_new_payer_30d"], 1)
GAP["activated_free_at_6pct"] = round(GAP["payers_to_close_gap"] / 0.06)
GAP["qualified_visitors_at_30pct_activation"] = round(GAP["activated_free_at_6pct"] / 0.30)


def write_csv(name, rows):
    with (ROOT / name).open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


write_csv("play_scoring.csv", PLAYS)
write_csv("rescue_economics.csv", ECONOMICS)


def markdown(source):
    return {"cell_type": "markdown", "metadata": {}, "source": source.strip()}


def code(source):
    return {
        "cell_type": "code",
        "execution_count": None,
        "metadata": {},
        "outputs": [],
        "source": source.strip(),
    }


notebook = {
    "nbformat": 4,
    "nbformat_minor": 5,
    "metadata": {
        "kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"},
        "language_info": {"name": "python", "version": "3"},
    },
    "cells": [
        markdown(
            """# DoodleVPN owned-acquisition plan — 31 July 2026

## tl;dr

Existing acquaintance audiences are exhausted, so the previous partner-first recommendation is invalid. The selected motion is **Doodle Rescue**: a one-gigabyte monthly emergency tier for new users, the already-built DoodleRay Network Doctor, and problem-led distribution through search, short video, and genuine community answers. Referral rewards remain a secondary multiplier.

The score is a transparent decision prior, not a forecast. The economics show the free tier breaks even before support at roughly 4.0% 30-day paid conversion when using the conservative fully allocated server cost of $0.24 per activated user-month."""
        ),
        markdown(
            """## Context & Methods

### Key Assumptions

- Internal aggregates cover business flows from 13 April through 30 July 2026.
- New non-referral payers produce about $6.03 of 30-day net revenue.
- Fully allocated infrastructure cost is about $0.24 per active user-month; marginal cost is unknown and likely lower.
- The free pilot is limited to genuinely new users from dedicated acquisition links, excluding active and prior payers to control cannibalization.
- Scores compare complete go-to-market plays against the owner's constraints; they do not claim causal lift."""
        ),
        code(
            """import csv
from pathlib import Path

ROOT = Path('analysis/doodlevpn-owned-acquisition-plan-2026-07-31')

def read_csv(name):
    with (ROOT / name).open(encoding='utf-8') as handle:
        return list(csv.DictReader(handle))

plays = read_csv('play_scoring.csv')
economics = read_csv('rescue_economics.csv')"""
        ),
        markdown("## Data"),
        code("plays[:5]"),
        code("economics"),
        markdown("## Results"),
        code(
            f"""gap = {json.dumps(GAP, ensure_ascii=False)}
break_even = 0.24 / 6.03
assert plays[0]['play'].startswith('Doodle Rescue')
assert 0.039 < break_even < 0.041
{{
    'winner': plays[0]['play'],
    'winner_score': float(plays[0]['score']),
    'break_even_paid_conversion': round(break_even, 4),
    'monthly_revenue_gap': gap['monthly_gap'],
    'payers_to_close_gap': gap['payers_to_close_gap'],
    'activated_free_users_needed_at_6pct': gap['activated_free_at_6pct'],
    'qualified_visitors_needed_at_30pct_activation': gap['qualified_visitors_at_30pct_activation'],
}}"""
        ),
        markdown(
            """## Takeaways

1. The immediate objective is not vanity registrations but positive 30-day contribution per acquired cohort.
2. At the conservative cost assumption, 6% free-to-paid conversion yields about $12 contribution per 100 activated free users before support and development; 3% loses about $6.
3. Closing the entire May-to-July revenue gap requires roughly 126 additional payers per month, or about 2,100 activated free users at 6% conversion. This cannot be promised immediately without paid reach.
4. The first pilot should stop or change if 500 activated new free users produce below 3% paid conversion, server cost exceeds $0.35 per free active month, or fraud/support load breaks the model.
5. If the permanent reserve tier fails, keep the Network Doctor and problem-capture distribution but revert the offer to the current three-day trial."""
        ),
    ],
}


def execute_notebook(document):
    namespace = {}
    execution_count = 0
    for cell in document["cells"]:
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
        if stdout.getvalue():
            outputs.append({"output_type": "stream", "name": "stdout", "text": stdout.getvalue()})
        if result is not None:
            outputs.append(
                {
                    "output_type": "execute_result",
                    "execution_count": execution_count,
                    "metadata": {},
                    "data": {"text/plain": repr(result)},
                }
            )
        cell["outputs"] = outputs


execute_notebook(notebook)
(ROOT / "doodlevpn_owned_acquisition_plan.ipynb").write_text(
    json.dumps(notebook, ensure_ascii=False, indent=2), encoding="utf-8"
)


def sql_literal(value):
    if value is None:
        return "NULL"
    if isinstance(value, bool):
        return "1" if value else "0"
    if isinstance(value, (int, float)):
        return str(value)
    return "'" + str(value).replace("'", "''") + "'"


def values_sql(name, rows, columns):
    values = ",\n".join(
        "(" + ", ".join(sql_literal(row[column]) for column in columns) + ")" for row in rows
    )
    return f"WITH {name}({', '.join(columns)}) AS (VALUES\n{values}\n) SELECT * FROM {name};"


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
                "30-day net revenue per non-referral payer = $6.03",
                "fully allocated server cost = approximately $0.24 per active user-month",
                "May revenue = $3,581.06; July through 30 July = $2,823.94",
            ],
        },
    },
    {
        "id": "decision_model",
        "label": "Owned-acquisition decision model",
        "path": "analysis/doodlevpn-owned-acquisition-plan-2026-07-31/play_scoring.csv",
        "query": {
            "description": "Weighted comparison of thirteen acquisition plays under the zero-budget and saturated-network constraints.",
            "engine": "SQLite/Python",
            "sql": values_sql(
                "play_snapshot",
                PLAYS,
                [
                    "rank",
                    "play",
                    "score",
                    "cold_pull",
                    "doodle_fit",
                    "zero_upfront",
                    "speed_to_evidence",
                    "scalability",
                    "economics_measurement",
                    "verdict",
                ],
            ),
            "metric_definitions": [
                "score = 25% cold-audience pull + 20% Doodle reuse fit + 15% zero-upfront cash + 15% speed to evidence + 15% scalability + 10% economics and measurement",
                "scores are explicit decision priors rather than observed causal lifts",
            ],
        },
    },
    {
        "id": "rescue_economics",
        "label": "Doodle Rescue pilot economics",
        "path": "analysis/doodlevpn-owned-acquisition-plan-2026-07-31/rescue_economics.csv",
        "query": {
            "description": "Economics per 100 activated new free users at three paid-conversion levels.",
            "engine": "SQLite/Python",
            "sql": values_sql(
                "rescue_economics_snapshot",
                ECONOMICS,
                [
                    "scenario",
                    "activated_free_users",
                    "paid_conversion",
                    "new_payers",
                    "revenue_30d",
                    "server_cost_30d",
                    "contribution_before_support",
                ],
            ),
            "metric_definitions": [
                "30-day revenue = activated free users × paid conversion × $6.03",
                "server cost = activated free users × $0.24 fully allocated monthly cost",
                "contribution is before support, development and cannibalization; pilot excludes prior payers",
            ],
        },
    },
    {
        "id": "pc_diagnostics",
        "label": "Existing DoodleRay Network Doctor implementation",
        "path": "src/components/v6/DiagnosticPanel.tsx",
    },
    {
        "id": "adguard_free",
        "label": "AdGuard VPN free-plan page",
        "href": "https://adguard-vpn.com/en/free-vpn.html",
    },
    {
        "id": "windscribe_free",
        "label": "Windscribe free Windows plan",
        "href": "https://windscribe.com/features/windows",
    },
    {
        "id": "proton_free",
        "label": "Proton VPN free plan",
        "href": "https://protonvpn.com/free-vpn/linux",
    },
    {
        "id": "windows_support",
        "label": "Microsoft Windows connectivity troubleshooting",
        "href": "https://support.microsoft.com/ru-RU/Windows/Experience/Connectivity-Networking/fix-wi-fi-connection-issues-in-windows",
    },
    {
        "id": "telegram_affiliate",
        "label": "Telegram Affiliate Programs",
        "href": "https://www.telegram.org/blog/affiliate-programs-ai-sticker-search",
    },
]


artifact = {
    "surface": "report",
    "manifest": {
        "version": 1,
        "surface": "report",
        "title": "DoodleVPN: собственный поток новых пользователей",
        "generatedAt": GENERATED_AT,
        "sources": sources,
        "blocks": [
            {"id": "title", "type": "markdown", "body": "# DoodleVPN: собственный поток новых пользователей"},
            {
                "id": "executive",
                "type": "markdown",
                "body": "## Executive Summary\n\n- **Старую партнёрскую гипотезу снимаем.** Знакомые уже выжали свои аудитории; это объясняет прошлую просадку, но не даёт масштабируемого будущего канала.\n- **Один рекомендуемый механизм — Doodle Rescue.** Новому человеку даётся 1 ГБ резервного VPN в месяц, один девайс и авто-локация; внутри уже существующего Windows-клиента его встречает Network Doctor. Холодный вход строится на конкретной боли: починить VPN/интернет и оставить рабочий резерв.\n- **Приток создают не кнопки в боте, а problem capture:** пять сильных страниц, короткие видео и честные ответы в сообществах, ведущие на персональный маршрут Rescue. Рефералка добавляет трафик только после того, как собственный вход уже заработал.\n- **Порог решения:** ориентир безубыточности около 4.0% free-to-paid за 30 дней. Масштабировать при 6%+, закрывать или менять оффер при <3% после 500 активированных новых пользователей.",
            },
            {
                "id": "root_cause",
                "type": "markdown",
                "body": "## Бизнес потерял внешний вход, а не средний чек\n\nС мая по июль выручка снизилась примерно на $757 в месяц, регистрации — с 892 до 420, тогда как средний платёж остался около $5. Пользовательское уточнение снимает надежду на реактивацию знакомых: их аудитории конечны и уже пройдены. Значит, реферальный UX, уведомления и winback остаются полезными множителями, но не отвечают на вопрос «откуда взять незнакомых людей».\n\nЧтобы вернуться к майской выручке только за счёт новых плательщиков, нужно примерно 126 дополнительных плательщиков в месяц. При 6% конверсии Rescue в оплату это около 2.1k активированных бесплатных пользователей, то есть примерно 7k квалифицированных посетителей при 30% visitor→activation. Это цель масштаба, а не обещание первого месяца.",
                "sourceId": "internal_growth",
            },
            {"id": "ranking_intro", "type": "markdown", "body": "## Из тринадцати вариантов выигрывает полный продуктовый вход\n\nНа графике сравниваются не отдельные кнопки, а целые go-to-market механизмы. Оценка учитывает способность тянуть холодную аудиторию, переиспользовать то, что Doodle уже умеет, работать без предоплаты, быстро дать сигнал, масштабироваться и считаться. Платные размещения показаны, но отсечены как невыполнимые при нулевом бюджете."},
            {"id": "ranking_chart", "type": "chart", "chartId": "play_ranking"},
            {"id": "ranking_table", "type": "table", "tableId": "play_table"},
            {
                "id": "why_rescue",
                "type": "markdown",
                "body": "## Почему Rescue сильнее очередного триала\n\n**Оффер решает другую задачу:** не «успей оценить VPN за три дня», а «держи рабочий резерв, который не исчезнет». Это легче сохранить, вспомнить и переслать в момент, когда у друга что-то сломалось. Ограничение в 1 ГБ и одном устройстве оставляет платному тарифу понятную работу: постоянное использование, все локации и устройства.\n\nFreemium здесь не копируется с гигантов буквально. AdGuard даёт 3 ГБ в месяц, Windscribe — 10 ГБ с подтверждённой почтой, Proton — безлимитный free. Их масштабы неприменимы к Doodle, но они подтверждают сам паттерн: постоянный ограниченный доступ может быть acquisition-бюджетом. Doodle начинает с гораздо более жёсткого 1 ГБ и закрытого пилота.",
            },
            {
                "id": "existing_asset",
                "type": "markdown",
                "body": "## Дифференциация уже частично построена\n\nВ Windows-клиенте есть Network Doctor: он классифицирует сломанный адаптер, мёртвый VPN-движок, проблемы службы, старый системный proxy, неподнятый browser proxy и другие состояния; часть причин умеет чинить автоматически. Поэтому первый MVP — не новый сложный продукт. Нужно вынести понятное обещание на публичные problem-страницы и привести человека в диагностику плюс Rescue-доступ.\n\n**Doctor не должен блокировать запуск:** если подписанный Windows-клиент ещё не готов для массового трафика, Rescue сначала работает через текущий бот и существующие способы подключения, а Windows-route включается после release gate.",
                "sourceId": "pc_diagnostics",
            },
            {
                "id": "offer",
                "type": "markdown",
                "body": "## Точный оффер пилота\n\n- Только новые пользователи с acquisition-ссылок; действующие и бывшие плательщики не переводятся на free.\n- 1 ГБ на 30 дней, один девайс, автоматическая локация, без P2P; после лимита соединение останавливается.\n- На 70% расхода — одна кнопка на актуальный платный тариф; на 100% — экран сравнения free и paid.\n- За приглашённого друга, который прожил 72 часа и использовал 100 МБ: обоим +500 МБ; максимум три бонуса за 30 дней. Денежные 20% после реальной оплаты остаются, но не являются главным обещанием.\n- Никаких кейсов, колёс, ежедневной тапалки и внешних призов в первые 60 дней.",
            },
            {
                "id": "anti_fraud",
                "type": "markdown",
                "body": "## Антифрод без слежки за нормальными людьми\n\nОдин free-entitlement на Telegram user и уже существующий device public key; один активный free-девайс; приглашение не засчитывается при совпадении device key или повторно созданном аккаунте. Бонус открывается только через 72 часа и 100 МБ реального трафика. Совпадение сети/IP — только риск-сигнал, а не автоматический бан, иначе пострадают семьи и общественный Wi-Fi. Лимит три бонуса в месяц делает массовый фарм экономически бессмысленным. Никаких выплат за регистрацию, установку или сам расход трафика.",
            },
            {
                "id": "economics_intro",
                "type": "markdown",
                "body": "## Экономика допускает дешёвый проверяемый пилот\n\nКонсервативный расчёт использует полностью распределённые $0.24 на активного пользователя в месяц, хотя маржинальная стоимость, вероятно, ниже. На 100 активированных free-пользователей 6% конверсии дают около $36 выручки и $12 contribution до поддержки и разработки. Это не доказывает модель заранее, но задаёт честные границы запуска."},
            {"id": "economics_table", "type": "table", "tableId": "economics_table"},
            {
                "id": "plan",
                "type": "markdown",
                "body": "## Железный план 7/30/60/90\n\n### Дни 1–3 — поставить счётчики и лимиты\n\n1. Сделать отдельный `source_id` для каждого материала и полный путь: landing → bot start → account → first connection → 100 MB → 70% quota → checkout → payment → refund → D30.\n2. Проверить технический hard cap трафика и максимально допустимое число free-active на текущих серверах. Первый общий потолок — 500 активированных пользователей или $120 распределённой серверной стоимости.\n3. Исключить всех, кто платил раньше, и запретить перенос free-квоты между аккаунтами.\n\n### Дни 4–7 — собрать Rescue MVP\n\n1. Добавить один тариф 1 ГБ/30 дней и два quota-экрана: 70% и 100%.\n2. Сделать одну страницу «Doodle Rescue»: бесплатный резерв; Windows-ветка дополнительно обещает диагностику, если клиент прошёл release gate.\n3. Сделать пять symptom-страниц: VPN подключён, но интернета нет; интернет пропал после VPN; работает только браузер; DNS/IP leak; VPN не подключается после обновления Windows.\n4. Каждый материал ведёт в отдельный deep link, а не на общий старт бота.\n\n### Дни 8–30 — вручную создать первые 500 входов\n\n1. На каждый symptom: одна нормальная статья, одно короткое видео и один чек-лист.\n2. Ежедневно отвечать минимум на пять свежих релевантных вопросов в сообществах и комментариях; давать ссылку только там, где Doctor или Rescue реально решает проблему.\n3. Публиковать три коротких problem-видео в неделю в VK Видео, Дзен, YouTube/Rutube и вести на конкретный symptom route.\n4. Раз в неделю оставлять только темы с лучшим переходом в first connection и оплату.\n\n### Дни 31–60 — удваивать найденный спрос\n\n1. Расширить только две лучшие проблемы до 20 материалов каждая: версии по Windows 10/11, браузеру, провайдеру и типу сбоя.\n2. Добавить Windows Store/каталоги и страницы установки как distribution hygiene.\n3. Запустить +500 МБ за проверенного друга как multiplier; не менять одновременно лимит free и платный оффер.\n\n### Дни 61–90 — купить скорость только из будущей выручки\n\n1. Показать доказанную Rescue-воронку незнакомым микроавторам и тематическим сообществам.\n2. Платить только после подтверждённой оплаты и возвратного hold. Это ускоритель уже работающей модели, не её фундамент.\n3. Масштабировать два лучших source×problem кластера; остальные закрыть.",
            },
            {
                "id": "decision_rules",
                "type": "markdown",
                "body": "## Правила scale/kill\n\n**Масштабировать после 500 активированных новых free:** paid D30 ≥6%; стоимость free-active ≤$0.30/месяц; 30-дневный contribution положительный; fraud/multi-account <8%; support ≤0.15 обращения на free-active.\n\n**Оставить как нишевой канал:** paid D30 3–6%, но отдельные problem routes прибыльны. Убрать слабые темы, не весь продукт.\n\n**Закрыть permanent free и оставить Doctor + trial:** paid D30 <3%; стоимость >$0.35; free заменяет покупки; нет source-кластера с положительным contribution после 500 активаций.\n\n**Нельзя оценивать успех по регистрациям.** Главная метрика — `(30-day net revenue − server cost − referral obligation − refunds) / unique acquired visitor`, отдельно по каждому source×content route.",
            },
            {
                "id": "not_do",
                "type": "markdown",
                "body": "## Что не делать\n\n- Не ставить старых знакомых или массовую рефералку в основу плана.\n- Не делать игру до появления стабильного холодного входа.\n- Не выдавать 3-дневный trial за сильное acquisition-предложение.\n- Не открывать free всей существующей базе: это ломает тест каннибализацией.\n- Не публиковать 100 AI-статей без ручной проверки и реальной пользы.\n- Не платить за регистрации, клики или использованные 100 МБ.\n- Не запускать одновременно free, большую скидку, игру и новую CRM-цепочку.\n- Не считать Telegram Mini App Store самостоятельным каналом; официальная affiliate-механика завязана на Stars-покупки.",
            },
            {
                "id": "questions",
                "type": "markdown",
                "body": "## Further Questions\n\n- Какой реальный маржинальный, а не распределённый, cost даёт ещё 1 ГБ трафика и ещё один free-active?\n- Можно ли backend-ом жёстко ограничить 1 ГБ, один девайс и P2P без ручных операций?\n- Какие пять Windows-причин чаще всего встречаются в поддержке Doodle и дают лучший материал для первых страниц?\n- Какой текущий тариф и цена должны показываться на 70% quota без выдуманного промокода?",
            },
            {
                "id": "caveats",
                "type": "markdown",
                "body": "## Caveats and Assumptions\n\nРекомендация имеет высокую относительную, но среднюю абсолютную уверенность: у Doodle пока нет эксперимента с permanent free и нет полной маржинальной модели трафика. Оценки вариантов — прозрачные priors, а не прогноз. Основной риск — free привлечёт халявщиков или заменит оплату; поэтому пилот закрыт для старых пользователей и имеет жёсткий потолок. Российские требования к публичному продвижению VPN нужно проверить на конкретных текстах и площадках, даже если владелец не считает этот риск блокирующим.",
            },
        ],
        "charts": [
            {
                "id": "play_ranking",
                "title": "Acquisition play scores",
                "subtitle": "Complete plays under zero-budget and saturated-network constraints; 0–100 decision prior.",
                "type": "bar",
                "dataset": "plays",
                "sourceId": "decision_model",
                "encodings": {
                    "x": {"field": "play", "type": "nominal", "label": "Play"},
                    "y": {"field": "score", "type": "quantitative", "label": "Score", "format": "number"},
                },
                "valueFormat": "number",
                "options": {"orientation": "horizontal", "valueLabels": True},
            }
        ],
        "tables": [
            {
                "id": "play_table",
                "title": "Thirteen acquisition plays",
                "subtitle": "Scores are comparative decision priors; feasibility is shown separately in the verdict.",
                "dataset": "plays",
                "sourceId": "decision_model",
                "defaultSort": {"field": "rank", "direction": "asc"},
                "columns": [
                    {"field": "rank", "label": "Rank", "format": "number"},
                    {"field": "play", "label": "Play"},
                    {"field": "score", "label": "Score", "format": "number"},
                    {"field": "verdict", "label": "Decision"},
                ],
            },
            {
                "id": "economics_table",
                "title": "Economics per 100 activated free users",
                "subtitle": "Thirty-day revenue at $6.03 per payer and conservative $0.24 server cost per activated user.",
                "dataset": "economics",
                "sourceId": "rescue_economics",
                "defaultSort": {"field": "paid_conversion", "direction": "asc"},
                "columns": [
                    {"field": "scenario", "label": "Scenario"},
                    {"field": "paid_conversion", "label": "Paid D30", "format": "percent"},
                    {"field": "new_payers", "label": "New payers", "format": "number"},
                    {"field": "revenue_30d", "label": "30d revenue", "format": "currency"},
                    {"field": "server_cost_30d", "label": "Server cost", "format": "currency"},
                    {"field": "contribution_before_support", "label": "Contribution", "format": "currency", "movement": True},
                ],
            },
        ],
    },
    "snapshot": {
        "version": 1,
        "status": "ready",
        "generatedAt": GENERATED_AT,
        "datasets": {"plays": PLAYS, "economics": ECONOMICS},
    },
    "sources": sources,
}

(ROOT / "artifact.json").write_text(json.dumps(artifact, ensure_ascii=False, indent=2), encoding="utf-8")
