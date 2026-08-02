import ast
import contextlib
import csv
import io
import json
from pathlib import Path


ROOT = Path(__file__).parent
ROOT.mkdir(parents=True, exist_ok=True)
GENERATED_AT = "2026-07-31T19:00:00+03:00"

ACTIVE_PAYERS = 1144
CURRENT_RECENT_INVITERS = 75
REFERRAL_PAID_CONVERSION = 0.351
REVENUE_PER_REFERRAL_PAYER_30D = 6.98
CURRENT_COMMISSION_RATE = 0.20
FULL_MONTH_NET_REVENUE = 3.13
SERVER_COST_USER_MONTH = 0.24


SCENARIOS = []
for name, target_rate, registrations_per_new_inviter in [
    ("Pessimistic", 0.10, 1.30),
    ("Base", 0.15, 1.50),
    ("Aggressive", 0.20, 1.70),
]:
    target_inviters = ACTIVE_PAYERS * target_rate
    incremental_inviters = max(0, target_inviters - CURRENT_RECENT_INVITERS)
    incremental_registrations = incremental_inviters * registrations_per_new_inviter
    incremental_payers = incremental_registrations * REFERRAL_PAID_CONVERSION
    revenue = incremental_payers * REVENUE_PER_REFERRAL_PAYER_30D
    commission = revenue * CURRENT_COMMISSION_RATE
    payer_server_cost = incremental_payers * SERVER_COST_USER_MONTH
    reward_infrastructure = incremental_payers * SERVER_COST_USER_MONTH
    extra_trial_infrastructure = incremental_registrations * SERVER_COST_USER_MONTH * 4 / 30
    max_opportunity_cost = incremental_payers * FULL_MONTH_NET_REVENUE
    cash_contribution = revenue - commission - payer_server_cost - reward_infrastructure - extra_trial_infrastructure
    SCENARIOS.append(
        {
            "scenario": name,
            "target_inviter_rate": round(target_rate, 3),
            "incremental_inviters": round(incremental_inviters, 1),
            "incremental_registrations": round(incremental_registrations, 1),
            "incremental_payers": round(incremental_payers, 1),
            "incremental_revenue_30d": round(revenue, 2),
            "current_20pct_obligation": round(commission, 2),
            "payer_server_cost": round(payer_server_cost, 2),
            "reward_cash_cost": round(reward_infrastructure + extra_trial_infrastructure, 2),
            "max_reward_opportunity_cost": round(max_opportunity_cost, 2),
            "contribution_after_max_opportunity": round(cash_contribution - max_opportunity_cost, 2),
        }
    )


FACTS = [
    {"metric": "Active payers", "value": 1144, "interpretation": "Addressable satisfied-user base"},
    {"metric": "Invited anyone in last 30d", "value": 75, "interpretation": "Only 6.6% of active payers"},
    {"metric": "Never invited anyone", "value": 821, "interpretation": "71.8% of active payers"},
    {"metric": "Zero active paid referrals", "value": 960, "interpretation": "Largest untapped group"},
    {"metric": "Stopped after 1–2 registrations", "value": 441, "interpretation": "72.4% of all historical referrers"},
    {"metric": "Ordinary referral 7d paid conversion", "value": 0.351, "interpretation": "High quality after a friend arrives"},
]


def write_csv(name, rows):
    with (ROOT / name).open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


write_csv("sprint_scenarios.csv", SCENARIOS)
write_csv("referral_facts.csv", FACTS)


def markdown(source):
    return {"cell_type": "markdown", "metadata": {}, "source": source.strip()}


def code(source):
    return {"cell_type": "code", "execution_count": None, "metadata": {}, "outputs": [], "source": source.strip()}


notebook = {
    "nbformat": 4,
    "nbformat_minor": 5,
    "metadata": {
        "kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"},
        "language_info": {"name": "python", "version": "3"},
    },
    "cells": [
        markdown(
            """# DoodleVPN referral sprint — 31 July 2026

## tl;dr

The scalable supply is not the exhausted audiences of a few acquaintances. It is the mostly inactive personal graph of the ordinary user base: only 6.6% of 1,144 active payers invited anyone in the last 30 days, while ordinary referred users convert to paid at about 35.1% in seven days.

The recommended 30-day treatment is one simple gift: **the friend gets seven trial days instead of three; the referrer gets 30 subscription days after the friend's first verified payment**. The existing 20% obligation stays during the experiment so the test can launch without rewriting old contracts. If it wins, new referrals should choose either days or money rather than receiving both forever."""
        ),
        markdown(
            """## Context & Methods

### Key Assumptions

- Business data covers flows since 13 April 2026 and closes July at 30 July.
- The scenario model targets ordinary active payers and excludes special partners and large outliers.
- Incremental inviter rates are assumptions, not forecasts.
- 30-day revenue per ordinary referred payer is $6.98.
- Opportunity cost treats every 30-day reward as if it fully displaced a one-month renewal worth $3.13; this is deliberately conservative.
- The reward's infrastructure cash cost is modeled separately at $0.24 per user-month."""
        ),
        code(
            """import csv
from pathlib import Path

ROOT = Path('analysis/doodlevpn-referral-sprint-2026-07-31')

def read_csv(name):
    with (ROOT / name).open(encoding='utf-8') as handle:
        return list(csv.DictReader(handle))

scenarios = read_csv('sprint_scenarios.csv')
facts = read_csv('referral_facts.csv')"""
        ),
        markdown("## Data"),
        code("facts"),
        code("scenarios"),
        markdown("## Results"),
        code(
            """base = next(row for row in scenarios if row['scenario'] == 'Base')
assert 50 <= float(base['incremental_payers']) <= 52
assert float(base['contribution_after_max_opportunity']) > 0
{
    'base_incremental_payers': float(base['incremental_payers']),
    'base_incremental_revenue_30d': float(base['incremental_revenue_30d']),
    'base_contribution_after_max_opportunity': float(base['contribution_after_max_opportunity']),
    'share_of_may_to_july_gap_recovered': round(float(base['incremental_revenue_30d']) / (3581.06 - 2823.94), 3),
}"""
        ),
        markdown(
            """## Takeaways

1. Referral quality is not the bottleneck; referral participation is.
2. A three-referral threshold is a bad launch default for Doodle because 72.4% of historical referrers stopped after one or two registrations.
3. The base scenario raises recent inviter participation from 6.6% to 15%, producing about 51 incremental payers and $356 of 30-day revenue.
4. Even after the current 20% commission and a worst-case full-month opportunity cost for every sender reward, all three scenarios remain contribution-positive before support and development.
5. The experiment should be judged on incremental first-paid referrals per eligible user and 30-day contribution, not shares or registrations."""
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
                result = eval(compile(ast.Expression(tree.body[-1].value), f"cell-{execution_count}", "eval"), namespace)
            else:
                exec(compile(tree, f"cell-{execution_count}", "exec"), namespace)
        outputs = []
        if stdout.getvalue():
            outputs.append({"output_type": "stream", "name": "stdout", "text": stdout.getvalue()})
        if result is not None:
            outputs.append({"output_type": "execute_result", "execution_count": execution_count, "metadata": {}, "data": {"text/plain": repr(result)}})
        cell["outputs"] = outputs


execute_notebook(notebook)
(ROOT / "doodlevpn_referral_sprint.ipynb").write_text(json.dumps(notebook, ensure_ascii=False, indent=2), encoding="utf-8")


def sql_literal(value):
    if isinstance(value, (int, float)):
        return str(value)
    return "'" + str(value).replace("'", "''") + "'"


def values_sql(name, rows, columns):
    body = ",\n".join("(" + ", ".join(sql_literal(row[column]) for column in columns) + ")" for row in rows)
    return f"WITH {name}({', '.join(columns)}) AS (VALUES\n{body}\n) SELECT * FROM {name};"


sources = [
    {
        "id": "internal_referral",
        "label": "DoodleVPN validated referral analysis",
        "path": "analysis/doodlevpn-growth-research-2026-07-30/doodlevpn_growth_research.ipynb",
        "query": {
            "description": "Validated ordinary referral behavior and economics from DoodleVPN data since 13 April 2026.",
            "engine": "SQLite/Python",
            "tables_used": ["bot_mirror.users", "bot_mirror.payments", "bot_mirror.referral_holds", "accounting.revenue"],
            "filters": ["Business flows >= 2026-04-13", "Special partners excluded where ordinary referrals are stated"],
            "metric_definitions": [
                "recent inviter rate = 75 active payers with an invite in 30 days / 1,144 active payers = 6.6%",
                "ordinary referral 7-day paid conversion in mature July cohort = 35.1%",
                "30-day net revenue per ordinary referred payer = $6.98",
            ],
        },
    },
    {
        "id": "scenario_model",
        "label": "Referral sprint scenario model",
        "path": "analysis/doodlevpn-referral-sprint-2026-07-31/sprint_scenarios.csv",
        "query": {
            "description": "Pessimistic, base and aggressive participation scenarios for a 30-day gift sprint.",
            "engine": "SQLite/Python",
            "sql": values_sql(
                "scenario_snapshot",
                SCENARIOS,
                [
                    "scenario",
                    "target_inviter_rate",
                    "incremental_inviters",
                    "incremental_registrations",
                    "incremental_payers",
                    "incremental_revenue_30d",
                    "current_20pct_obligation",
                    "payer_server_cost",
                    "reward_cash_cost",
                    "max_reward_opportunity_cost",
                    "contribution_after_max_opportunity",
                ],
            ),
            "metric_definitions": [
                "incremental payers = incremental inviters × registrations per inviter × 35.1% paid conversion",
                "payer server cost and reward server cost are shown as separate $0.24 user-months",
                "cash reward cost includes the referrer's reward month plus four added friend trial days",
                "max opportunity cost assumes every 30-day sender reward replaces $3.13 of renewal revenue",
            ],
        },
    },
    {"id": "in_kind_study", "label": "Monetary versus in-kind referral rewards", "href": "https://www.sciencedirect.com/science/article/pii/S0167811613000906"},
    {"id": "uncertainty_study", "label": "Management Science referral field experiment", "href": "https://pubsonline.informs.org/doi/10.1287/mnsc.2024.05685"},
    {"id": "freemium_study", "label": "Social Referral Programs for Freemium Platforms", "href": "https://pubsonline.informs.org/doi/10.1287/mnsc.2022.4301"},
]


artifact = {
    "surface": "report",
    "manifest": {
        "version": 1,
        "surface": "report",
        "title": "DoodleVPN: 30-дневный реферальный толчок",
        "generatedAt": GENERATED_AT,
        "sources": sources,
        "blocks": [
            {"id": "title", "type": "markdown", "body": "# DoodleVPN: 30-дневный реферальный толчок"},
            {
                "id": "executive",
                "type": "markdown",
                "body": "## Executive Summary\n\n- **Да: основу сейчас надо делать на массовой рефералке, но не на знакомых-партнёрах.** У 1,144 активных плательщиков собственные маленькие социальные графы; за последние 30 дней приглашали только 75 человек.\n- **Запускать один оффер:** «Другу — 7 дней вместо 3. Вам — 30 дней после его первой оплаты». Награда даётся уже за первого платящего друга; никакого порога в три человека.\n- **На 30 дней не ломать текущие 20%.** Treatment получает новую награду сверху, control остаётся на старой механике. Если пакет выигрывает, для новых referrals постоянная модель становится выбором «30 дней или 20%», без вечного двойного вознаграждения.\n- **Базовый сценарий:** рост доли недавно реферивших с 6.6% до 15% даёт около 51 дополнительного плательщика и $356 новой 30-дневной выручки — примерно 47% майско-июльского разрыва.",
            },
            {
                "id": "source",
                "type": "markdown",
                "body": "## Источник новых людей — 93.4% базы, которая сейчас молчит\n\nПартнёрские аудитории действительно исчерпаны. Но это не означает, что исчерпаны личные графы обычных пользователей: 71.8% активных плательщиков вообще никогда никого не приглашали, 960 не имеют ни одного активного платного реферала, а за последний месяц приглашали лишь 6.6%.\n\nКачество уже доказано: обычные рефералы конвертируются в оплату примерно вдвое лучше нереферальных регистраций. Значит, bottleneck находится до перехода друга — пользователь не видит достаточно сильной и социально нормальной причины отправить ссылку.",
                "sourceId": "internal_referral",
            },
            {
                "id": "offer",
                "type": "markdown",
                "body": "## Единственный оффер теста\n\n**Друг получает точно семь бесплатных дней вместо стандартных трёх. Реферер получает точно 30 дней подписки после первой подтверждённой оплаты друга.**\n\nПочему не $0.63 и не порог «три друга»: деньги создают ощущение, что пользователь зарабатывает на знакомом, а маленький баланс и $10 на вывод слишком далеки. Исследования находят, что для менее сильного бренда in-kind reward может работать лучше денег из-за меньшего социального дискомфорта; крупный field experiment показывает, что получателю важна определённая, а не случайная выгода. Doodle-данные добавляют решающий аргумент: 72.4% исторических рефереров остановились на одном-двух приглашённых, поэтому threshold создаст cold start.\n\nПоказывать честно: «Тебе 7 дней. Если останешься — мне добавят месяц». Это подарок с прозрачной взаимной выгодой, а не MLM.",
            },
            {
                "id": "mobile_flow",
                "type": "markdown",
                "body": "## Мобильный flow в пять экранов\n\n1. На главном экране после успешного подключения или оплаты — карточка **«Подарить другу 7 дней»**. Не ещё одна постоянная кнопка в общей каше.\n2. Реферальный экран: один тезис «Другу 7 дней · вам 30 дней после его оплаты», одна кнопка **«Подарить»**, ниже статусы друзей.\n3. Telegram share sheet с текстом: «Я пользуюсь DoodleVPN. По моей ссылке тебе 7 дней вместо 3. Если останешься — мне тоже добавят месяц: …».\n4. Friend landing: «[Имя] подарил вам 7 дней DoodleVPN», одна кнопка **«Подключить бесплатно»**. Не показывать заработок, USDT и длинные условия до подключения.\n5. После оплаты друга: «Готово — вам добавлено 30 дней». Баланс денег и вывод от $10 остаются отдельным вторичным блоком **«Зарабатывать»**.",
            },
            {
                "id": "economics_intro",
                "type": "markdown",
                "body": "## Даже консервативная экономика положительная\n\nМодель ниже оставляет текущие 20% и одновременно считает каждый подаренный месяц как будто он полностью заменил обычное продление на $3.13. Это завышает opportunity cost, но даже при таком подходе все три сценария остаются положительными. Серверная cash-cost подарочного месяца показана отдельно и составляет около $0.24.",
            },
            {"id": "economics_chart", "type": "chart", "chartId": "scenario_chart"},
            {"id": "economics_table", "type": "table", "tableId": "scenario_table"},
            {
                "id": "experiment",
                "type": "markdown",
                "body": "## Эксперимент: 70% treatment, 30% holdout\n\n**Кого включить:** обычные пользователи, которые сейчас платят; исключить специальных партнёров, сотрудников, тестовые аккаунты и крупных outlier-рефереров. Randomization — по `referrer_user_id`, ссылка навсегда наследует arm.\n\n**Treatment:** 7 дней другу + 30 дней рефереру после первой оплаты + текущие 20% на время спринта. **Control:** текущий оффер 3 дня + 20%.\n\n**Primary:** first-paid referrals D30 / eligible users. **Secondary:** referral-page open, share, friend start, first connection, first paid, D30 net revenue. **Guardrails:** refunds/fraud, renewal displacement, bot blocks, support load.\n\nПри маленькой выборке использовать Bayesian/Poisson readout: пакет выигрывает, если вероятность положительного lift >80%, rate treatment минимум в 1.5 раза выше control и incremental contribution положительный. Не ждать формального p<0.05, которого эта база может не дать.",
            },
            {
                "id": "steps",
                "type": "markdown",
                "body": "## Пошагово: что делать завтра и дальше\n\n### Завтра\n\n1. Остановить любые планы игры, free-tier и порога «три друга».\n2. Зафиксировать treatment/control и оффер без вариантов.\n3. Добавить события: referral_view, share_tap, recipient_start, first_connection, first_paid, reward_granted, refund.\n4. Сохранить первую атрибуцию друга; повторные ссылки не перетирают referrer.\n\n### Дни 2–4\n\n1. Переписать один мобильный referral screen и friend landing.\n2. Добавить reward ledger: 30 дней выдаются один раз после settled payment и hold; при refund не выдавать или откатывать до использования.\n3. Антифрод: новый recipient, один trial lifetime, совпадающий Telegram/device key не засчитывается, сотрудники/тесты исключены. IP — только сигнал, не бан.\n\n### День 5\n\n1. Запустить на 10% treatment как QA.\n2. Проверить вручную полный путь двумя чистыми аккаунтами: share → start → connection → payment → 30 days.\n3. Если ledger и attribution сходятся — расширить до 70/30.\n\n### Дни 6–30\n\n1. Показывать карточку после успешной оплаты/подключения и в момент, когда пользователь явно доволен.\n2. Одно стартовое сообщение treatment-аудитории. Второе — только открывшим referral screen, но не нажавшим Share, через 72 часа. Третье — только когда друг начал trial или до награды остался один шаг.\n3. Не долбить ежедневными referral-уведомлениями. Максимум три кампанийных касания за 30 дней.\n4. Каждые семь дней считать rate и contribution, но не менять оффер посередине.",
            },
            {
                "id": "decision",
                "type": "markdown",
                "body": "## Решение на 30-й день\n\n**Rollout:** treatment даёт ≥1.5× first-paid rate control, минимум 30 дополнительных плательщиков в пересчёте на всю базу, положительный contribution, fraud/refund <8%. Для новых ссылок сделать два режима: по умолчанию **«Бесплатный VPN — 30 дней за каждого платящего друга»**; отдельно **«Заработок — 20% и вывод от $10»**. Старые обязательства 20% сохранить.\n\n**Повторить ещё 30 дней:** lift 1.2–1.5× и contribution положительный, но credible interval широкий. Менять только одну часть: сначала проверить 14 против 30 дней рефереру.\n\n**Kill:** lift <20%, меньше 10 дополнительных плательщиков, contribution отрицательный или fraud/refund ≥8%. Тогда не строить игру: вернуться к текущим 20% и искать внешний канал, потому что одной перестановкой UX социальные графы не расширить.",
            },
            {
                "id": "caveats",
                "type": "markdown",
                "body": "## Caveats and Assumptions\n\n51 дополнительный плательщик — сценарий, а не прогноз. Его главный допуск: новая механика поднимет долю недавно приглашающих с 6.6% до 15%. Control нужен именно для проверки этого допущения. Расчёт разделяет cash-cost, существующие 20% и максимальную упущенную выручку подаренного месяца; их нельзя складывать как одну и ту же статью. Внешние исследования проведены не на российском VPN, поэтому дизайн опирается прежде всего на собственную воронку Doodle.",
            },
        ],
        "charts": [
            {
                "id": "scenario_chart",
                "title": "Referral sprint scenarios",
                "subtitle": "Incremental first-paid users and 30-day revenue as recent-inviter participation rises from 6.6%.",
                "type": "bar",
                "dataset": "scenarios",
                "sourceId": "scenario_model",
                "encodings": {
                    "x": {"field": "scenario", "type": "nominal", "label": "Scenario"},
                    "y": {"field": "incremental_payers", "type": "quantitative", "label": "Incremental payers", "format": "number"},
                },
                "valueFormat": "number",
                "options": {"orientation": "vertical", "valueLabels": True},
            }
        ],
        "tables": [
            {
                "id": "scenario_table",
                "title": "Thirty-day scenario economics",
                "subtitle": "Current 20% obligation and maximum renewal displacement are both included during the temporary sprint.",
                "dataset": "scenarios",
                "sourceId": "scenario_model",
                "defaultSort": {"field": "target_inviter_rate", "direction": "asc"},
                "columns": [
                    {"field": "scenario", "label": "Scenario"},
                    {"field": "target_inviter_rate", "label": "Recent inviters", "format": "percent"},
                    {"field": "incremental_payers", "label": "New payers", "format": "number"},
                    {"field": "incremental_revenue_30d", "label": "30d revenue", "format": "currency"},
                    {"field": "current_20pct_obligation", "label": "Existing 20%", "format": "currency"},
                    {"field": "payer_server_cost", "label": "Payer server cost", "format": "currency"},
                    {"field": "reward_cash_cost", "label": "Reward cash cost", "format": "currency"},
                    {"field": "max_reward_opportunity_cost", "label": "Max opportunity cost", "format": "currency"},
                    {"field": "contribution_after_max_opportunity", "label": "Contribution", "format": "currency", "movement": True},
                ],
            }
        ],
    },
    "snapshot": {"version": 1, "status": "ready", "generatedAt": GENERATED_AT, "datasets": {"scenarios": SCENARIOS}},
    "sources": sources,
}

(ROOT / "artifact.json").write_text(json.dumps(artifact, ensure_ascii=False, indent=2), encoding="utf-8")
