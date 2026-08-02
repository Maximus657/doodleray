from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path


OUT = Path(__file__).resolve().parent
TITLE = "DoodleVPN: платить за подключение или за оплату"

cohort_rows = [
    {
        "cohort": "2026-04-13…04-30",
        "registrations": 142,
        "activation_proxy": 104,
        "activation_rate_pct": 73.2,
        "payers_d30": 38,
        "reg_to_paid_d30_pct": 26.8,
        "activation_to_paid_d30_pct": 24.0,
        "revenue_d30_rub": 18485,
    },
    {
        "cohort": "2026-05",
        "registrations": 250,
        "activation_proxy": 186,
        "activation_rate_pct": 74.4,
        "payers_d30": 89,
        "reg_to_paid_d30_pct": 35.6,
        "activation_to_paid_d30_pct": 34.4,
        "revenue_d30_rub": 43437,
    },
    {
        "cohort": "2026-06",
        "registrations": 208,
        "activation_proxy": 155,
        "activation_rate_pct": 74.5,
        "payers_d30": 86,
        "reg_to_paid_d30_pct": 41.3,
        "activation_to_paid_d30_pct": 43.2,
        "revenue_d30_rub": 41918,
    },
    {
        "cohort": "Всего",
        "registrations": 600,
        "activation_proxy": 445,
        "activation_rate_pct": 74.2,
        "payers_d30": 213,
        "reg_to_paid_d30_pct": 35.5,
        "activation_to_paid_d30_pct": 35.1,
        "revenue_d30_rub": 103841,
    },
]

first_payment_rows = [
    {
        "cohort": "2026-04-13…04-30",
        "payers": 38,
        "first_payment_revenue_rub": 17639,
        "current_20pct_rub": 3528,
        "fixed_100_paid_rub": 3800,
        "minimum_100_or_20pct_rub": 4880,
    },
    {
        "cohort": "2026-05",
        "payers": 89,
        "first_payment_revenue_rub": 40139,
        "current_20pct_rub": 8028,
        "fixed_100_paid_rub": 8900,
        "minimum_100_or_20pct_rub": 11274,
    },
    {
        "cohort": "2026-06",
        "payers": 86,
        "first_payment_revenue_rub": 40448,
        "current_20pct_rub": 8090,
        "fixed_100_paid_rub": 8600,
        "minimum_100_or_20pct_rub": 10546,
    },
    {
        "cohort": "Всего",
        "payers": 213,
        "first_payment_revenue_rub": 98227,
        "current_20pct_rub": 19645,
        "fixed_100_paid_rub": 21300,
        "minimum_100_or_20pct_rub": 26701,
    },
]

model_rows = [
    {
        "model": "Текущие 20% первой оплаты",
        "reward_cost_cohort_rub": 19645,
        "reward_per_registration_rub": 32.7,
        "revenue_after_reward_per_registration_rub": 140.3,
        "reward_cost_per_d30_payer_rub": 92.2,
        "extra_registrations_needed_vs_paid100_pct": -2.0,
        "verdict": "Control",
    },
    {
        "model": "100 ₽ за первую оплату",
        "reward_cost_cohort_rub": 21300,
        "reward_per_registration_rub": 35.5,
        "revenue_after_reward_per_registration_rub": 137.6,
        "reward_cost_per_d30_payer_rub": 100.0,
        "extra_registrations_needed_vs_paid100_pct": 0.0,
        "verdict": "Победитель",
    },
    {
        "model": "20 ₽ за активацию + 80 ₽ за оплату",
        "reward_cost_cohort_rub": 25940,
        "reward_per_registration_rub": 43.2,
        "revenue_after_reward_per_registration_rub": 129.8,
        "reward_cost_per_d30_payer_rub": 121.8,
        "extra_registrations_needed_vs_paid100_pct": 6.0,
        "verdict": "Второй тест",
    },
    {
        "model": "100 ₽ за активацию (proxy)",
        "reward_cost_cohort_rub": 44500,
        "reward_per_registration_rub": 74.2,
        "revenue_after_reward_per_registration_rub": 98.9,
        "reward_cost_per_d30_payer_rub": 208.9,
        "extra_registrations_needed_vs_paid100_pct": 39.1,
        "verdict": "Не запускать",
    },
]

connection_sensitivity_rows = [
    {
        "verified_connection_rate_pct": 40,
        "reward_cost_per_registration_rub": 40,
        "revenue_after_reward_per_registration_rub": 133.1,
        "registration_lift_needed_vs_paid100_pct": 3.4,
    },
    {
        "verified_connection_rate_pct": 50,
        "reward_cost_per_registration_rub": 50,
        "revenue_after_reward_per_registration_rub": 123.1,
        "registration_lift_needed_vs_paid100_pct": 11.8,
    },
    {
        "verified_connection_rate_pct": 60,
        "reward_cost_per_registration_rub": 60,
        "revenue_after_reward_per_registration_rub": 113.1,
        "registration_lift_needed_vs_paid100_pct": 21.7,
    },
    {
        "verified_connection_rate_pct": 74.2,
        "reward_cost_per_registration_rub": 74.2,
        "revenue_after_reward_per_registration_rub": 98.9,
        "registration_lift_needed_vs_paid100_pct": 39.1,
    },
]

quality_rows = [
    {
        "signal": "trial_used / trial_started",
        "meaning": "Пользователь активировал trial",
        "quality": "Средняя",
        "problem": "Не доказывает реальный VPN-сеанс; старые trial_started неполны",
        "reward_ready": "Нет",
    },
    {
        "signal": "guide_connected",
        "meaning": "Нажал «готово» в инструкции",
        "quality": "Низкая",
        "problem": "Самодекларация, легко фармится",
        "reward_ready": "Нет",
    },
    {
        "signal": "remnawave first_connected_at",
        "meaning": "В snapshot виден первый connect",
        "quality": "Низкая исторически",
        "problem": "Snapshot неполный и цензурированный; часть плательщиков отсутствует",
        "reward_ready": "Нет",
    },
    {
        "signal": "первая successful payment + hold",
        "meaning": "Получены деньги, refund window пройден",
        "quality": "Высокая",
        "problem": "Позже, чем connect, зато причинно связано с кассой",
        "reward_ready": "Да",
    },
]

plan_rows = [
    {
        "order": 1,
        "when": "День 0",
        "action": "Зафиксировать новые ordinary referrals в 50/50: текущие 20% против фиксированных 100 ₽ за первую successful payment; далее 20% в обеих группах.",
        "gate": "Старые связи и special partners не менять",
    },
    {
        "order": 2,
        "when": "Дни 1–3",
        "action": "Показать treatment: «100 ₽ за первую оплату друга. 3 друга = месяц VPN». Награда pending 48 часов, refund отменяет её.",
        "gate": "Не платить за signup, trial или кнопку «подключил»",
    },
    {
        "order": 3,
        "when": "Дни 1–3",
        "action": "Записать referral_view, share_click, friend_start, first_paid, reward_pending, reward_available, reward_spent и variant.",
        "gate": "≥95% первых оплат имеют referrer_id и variant",
    },
    {
        "order": 4,
        "when": "Дни 4–34",
        "action": "Считать D30 contribution на eligible referrer: выручка друзей минус награды, refunds и платежные комиссии.",
        "gate": "Не менять одновременно trial, цену, checkout и уведомления",
    },
    {
        "order": 5,
        "when": "После ≥30 treatment first-paid",
        "action": "Раскатить, если paid/referrer и contribution/referrer выше control; закрыть, если contribution не вырос.",
        "gate": "Fraud/refund ≤5%; бонусная liability полностью зарезервирована",
    },
    {
        "order": 6,
        "when": "Только если paid-trigger выиграл",
        "action": "Отдельно проверить 20 ₽ за verified connection + 80 ₽ за оплату.",
        "gate": "Нужен append-only verified connection event; требуется ≥6% signup lift",
    },
]


def markdown(source: str) -> dict:
    return {"cell_type": "markdown", "metadata": {}, "source": source.splitlines(keepends=True)}


def code(source: str) -> dict:
    return {"cell_type": "code", "execution_count": None, "metadata": {}, "outputs": [], "source": source.splitlines(keepends=True)}


def execute(cells: list[dict]) -> None:
    scope: dict = {}
    count = 0
    for cell in cells:
        if cell["cell_type"] != "code":
            continue
        count += 1
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            exec(compile("".join(cell["source"]), f"cell-{count}", "exec"), scope)
        cell["execution_count"] = count
        cell["outputs"] = [{"name": "stdout", "output_type": "stream", "text": output.getvalue().splitlines(keepends=True)}]


def build_notebook() -> None:
    cells = [
        markdown(
            "# DoodleVPN referral trigger analysis\n\n"
            "**Решение:** платить за первую подтверждённую оплату, не за подключение. "
            "На 600 зрелых регистрациях paid-trigger стоит 21 300 ₽, а ранний activation-proxy — 44 500 ₽."
        ),
        markdown(
            "## Method\n\n"
            "Обычные рефералы с комиссией 20%, регистрации 13 апреля–30 июня 2026, "
            "30-дневное окно оплаты. `trial_used` используется только как широкий proxy ранней активации; "
            "это не достоверный реальный VPN-connect."
        ),
        code(
            f"cohorts = {json.dumps(cohort_rows, ensure_ascii=False)}\n"
            f"models = {json.dumps(model_rows, ensure_ascii=False)}\n"
            "total = cohorts[-1]\n"
            "assert total['registrations'] == 600\n"
            "assert total['payers_d30'] == 213\n"
            "assert round(total['payers_d30'] / total['registrations'] * 100, 1) == 35.5\n"
            "for row in models:\n"
            "    print(row['model'], row['reward_cost_cohort_rub'], row['verdict'])"
        ),
        markdown(
            "## Result\n\n"
            "- Registration → D30 paid: **35.5%**.\n"
            "- Activation proxy: **74.2%**; activation proxy → D30 paid: **35.1%**.\n"
            "- 100 ₽ paid-trigger costs **35.5 ₽ per registration**.\n"
            "- 100 ₽ activation-trigger costs **74.2 ₽ per registration**.\n"
            "- At observed rates, activation-trigger needs **39.1% more registrations** merely to match paid-trigger contribution."
        ),
    ]
    execute(cells)
    notebook = {
        "cells": cells,
        "metadata": {
            "kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"},
            "language_info": {"name": "python", "version": "3"},
        },
        "nbformat": 4,
        "nbformat_minor": 5,
    }
    (OUT / "doodlevpn_referral_trigger_analysis.ipynb").write_text(
        json.dumps(notebook, ensure_ascii=False, indent=1), encoding="utf-8"
    )


def table(table_id: str, title: str, dataset: str, source: dict, sort_field: str, columns: list[dict], subtitle: str = "") -> dict:
    return {
        "id": table_id,
        "title": title,
        "subtitle": subtitle,
        "dataset": dataset,
        "sourceId": source["id"],
        "source": source,
        "defaultSort": {"field": sort_field, "direction": "asc"},
        "columns": columns,
    }


def build_artifact() -> None:
    cohort_source = {
        "id": "cohort_sql",
        "label": "Production SQLite: mature ordinary-referral cohorts",
        "query": {
            "description": "D30 conversion and revenue for ordinary referred registrations from April 13 through June 30, 2026.",
            "engine": "SQLite",
            "tables_used": ["users", "payments"],
            "filters": ["child created_at >= 2026-04-13 and < 2026-07-01", "referrer commission = 20%", "successful payments in first 30 days"],
            "metric_definitions": [
                "activation proxy = users.trial_used; not a verified VPN connection",
                "D30 payer = at least one successful payment within 30 days of registration",
                "D30 revenue = successful RUB payments within 30 days",
            ],
            "sql": "WITH base AS (SELECT u.tg_id,u.created_at,u.trial_used FROM users u JOIN users r ON r.tg_id=u.referrer_id WHERE u.created_at>='2026-04-13' AND u.created_at<'2026-07-01' AND COALESCE(r.ref_commission_pct,20)=20), rev AS (SELECT b.tg_id,SUM(CASE WHEN p.status IN('success','completed','paid','settled') AND p.created_at<datetime(b.created_at,'+30 days') AND upper(p.currency)='RUB' THEN p.amount ELSE 0 END) rev30,MAX(CASE WHEN p.status IN('success','completed','paid','settled') AND p.created_at<datetime(b.created_at,'+30 days') THEN 1 ELSE 0 END) paid30 FROM base b LEFT JOIN payments p ON p.tg_id=b.tg_id GROUP BY b.tg_id) SELECT COUNT(*) registrations,SUM(trial_used) activation_proxy,SUM(paid30) payers30,SUM(rev30) revenue30 FROM base JOIN rev USING(tg_id);",
        },
    }
    payment_source = {
        "id": "first_payment_sql",
        "label": "Production SQLite: first-payment reward replay",
        "query": {
            "description": "Replay current 20%, fixed 100 RUB, and minimum 100 RUB reward rules on each payer's first D30 payment.",
            "engine": "SQLite",
            "tables_used": ["users", "payments"],
            "filters": ["same mature ordinary-referral cohort", "first successful RUB payment only"],
            "metric_definitions": ["fixed 100 = 100 RUB per first payer", "minimum 100 or 20% = max(100, 20% of first payment)"],
            "sql": "WITH base AS (SELECT u.tg_id,u.created_at FROM users u JOIN users r ON r.tg_id=u.referrer_id WHERE u.created_at>='2026-04-13' AND u.created_at<'2026-07-01' AND COALESCE(r.ref_commission_pct,20)=20), ranked AS (SELECT b.tg_id,p.amount,ROW_NUMBER() OVER(PARTITION BY b.tg_id ORDER BY p.created_at,p.id) rn FROM base b JOIN payments p ON p.tg_id=b.tg_id WHERE p.status IN('success','completed','paid','settled') AND p.created_at<datetime(b.created_at,'+30 days') AND upper(p.currency)='RUB') SELECT COUNT(*) payers,SUM(amount) first_revenue,.2*SUM(amount) current_20pct,100*COUNT(*) fixed_100,SUM(MAX(100,.2*amount)) minimum_100_or_20pct FROM ranked WHERE rn=1;",
        },
    }
    model_source = {
        "id": "decision_math",
        "label": "Referral trigger unit-economics model",
        "query": {
            "description": "Reward cost and D30 revenue after reward per referred registration; common infrastructure, processor fees, and future renewals excluded equally.",
            "engine": "SQLite",
            "metric_definitions": [
                "revenue per registration = 103841.34 / 600 = 173.1 RUB",
                "paid-trigger cost per registration = 100 × 213 / 600 = 35.5 RUB",
                "activation-trigger cost per registration = 100 × 445 / 600 = 74.2 RUB",
                "required signup lift = paid-trigger contribution / alternative contribution − 1",
            ],
            "sql": "WITH a(regs,activated,payers,revenue,current_reward) AS (VALUES(600.0,445.0,213.0,103841.34,19645.43)) SELECT 100*payers/regs paid100_per_reg,100*activated/regs activation100_per_reg,revenue/regs-100*payers/regs paid100_after_reward,revenue/regs-100*activated/regs activation100_after_reward,(revenue/regs-100*payers/regs)/(revenue/regs-100*activated/regs)-1 required_signup_lift FROM a;",
        },
    }
    quality_source = {
        "id": "telemetry_audit",
        "label": "Connection-trigger telemetry audit",
        "query": {
            "description": "Semantic audit of current activation and connection signals.",
            "engine": "SQLite + code inspection",
            "tables_used": ["users", "user_events", "remnawave_user_latest"],
            "metric_definitions": ["reward-ready means append-only, server-verifiable, historically complete enough for payout"],
            "sql": "SELECT u.trial_used,u.trial_started_at,e.event_name,r.first_connected_at,r.last_connected_at FROM users u LEFT JOIN user_events e ON e.tg_id=u.tg_id AND e.event_name='guide_connected' LEFT JOIN remnawave_user_latest r ON r.tg_id=u.tg_id;",
        },
    }
    plan_source = {
        "id": "experiment_plan",
        "label": "Controlled referral-trigger experiment",
        "query": {
            "description": "Six-step sequential experiment plan.",
            "engine": "SQLite",
            "metric_definitions": ["primary decision metric = D30 contribution per eligible referrer"],
            "sql": "WITH steps(step_no) AS (VALUES(1),(2),(3),(4),(5),(6)) SELECT * FROM steps ORDER BY step_no;",
        },
    }
    sources = [cohort_source, payment_source, model_source, quality_source, plan_source]
    chart = {
        "id": "reward_cost_per_registration",
        "title": "Стоимость награды на одну реферальную регистрацию",
        "subtitle": "Ранний trigger оплачивает много неплательщиков; 100 ₽ после оплаты почти совпадают с текущими 20%.",
        "type": "bar",
        "dataset": "model_rows",
        "sourceId": model_source["id"],
        "source": model_source,
        "encodings": {
            "x": {"field": "model", "type": "nominal", "label": "Модель"},
            "y": {"field": "reward_per_registration_rub", "type": "quantitative", "label": "₽ на регистрацию"},
        },
    }
    tables = [
        table(
            "cohorts",
            "Помесячная конверсия ordinary referrals",
            "cohort_rows",
            cohort_source,
            "cohort",
            [
                {"field": "cohort", "label": "Когорта"},
                {"field": "registrations", "label": "Регистрации", "format": "number"},
                {"field": "activation_proxy", "label": "Activation proxy", "format": "number"},
                {"field": "activation_rate_pct", "label": "Activation, %", "format": "number"},
                {"field": "payers_d30", "label": "D30 плательщики", "format": "number"},
                {"field": "reg_to_paid_d30_pct", "label": "Reg→paid D30, %", "format": "number"},
                {"field": "activation_to_paid_d30_pct", "label": "Activation→paid D30, %", "format": "number"},
                {"field": "revenue_d30_rub", "label": "D30 выручка, ₽", "format": "number"},
            ],
            "Регистрации 13 апреля–30 июня; 30-дневное окно дозревания.",
        ),
        table(
            "first_payments",
            "Фактическая стоимость награды на первых оплатах",
            "first_payment_rows",
            payment_source,
            "cohort",
            [
                {"field": "cohort", "label": "Когорта"},
                {"field": "payers", "label": "Плательщики", "format": "number"},
                {"field": "first_payment_revenue_rub", "label": "Первые оплаты, ₽", "format": "number"},
                {"field": "current_20pct_rub", "label": "Текущие 20%, ₽", "format": "number"},
                {"field": "fixed_100_paid_rub", "label": "100 ₽ за paid, ₽", "format": "number"},
                {"field": "minimum_100_or_20pct_rub", "label": "max(100 ₽, 20%), ₽", "format": "number"},
            ],
        ),
        table(
            "models",
            "Сравнение моделей",
            "model_rows",
            model_source,
            "reward_per_registration_rub",
            [
                {"field": "model", "label": "Модель"},
                {"field": "reward_cost_cohort_rub", "label": "Награды на 600, ₽", "format": "number"},
                {"field": "reward_per_registration_rub", "label": "₽ / регистрация", "format": "number"},
                {"field": "revenue_after_reward_per_registration_rub", "label": "D30 выручка после награды / reg, ₽", "format": "number"},
                {"field": "reward_cost_per_d30_payer_rub", "label": "Награда / D30 payer, ₽", "format": "number"},
                {"field": "extra_registrations_needed_vs_paid100_pct", "label": "Нужный signup lift vs paid100, %", "format": "number"},
                {"field": "verdict", "label": "Вердикт"},
            ],
        ),
        table(
            "sensitivity",
            "Когда 100 ₽ за реальный connect могут окупиться",
            "connection_sensitivity_rows",
            model_source,
            "verified_connection_rate_pct",
            [
                {"field": "verified_connection_rate_pct", "label": "Connect rate, %", "format": "number"},
                {"field": "reward_cost_per_registration_rub", "label": "Награда / reg, ₽", "format": "number"},
                {"field": "revenue_after_reward_per_registration_rub", "label": "После награды / reg, ₽", "format": "number"},
                {"field": "registration_lift_needed_vs_paid100_pct", "label": "Нужный signup lift, %", "format": "number"},
            ],
            "Чтобы сравняться с 100 ₽ за first paid при неизменной D30 выручке на регистрацию.",
        ),
        table(
            "quality",
            "Можно ли сейчас честно платить за connect",
            "quality_rows",
            quality_source,
            "signal",
            [
                {"field": "signal", "label": "Сигнал"},
                {"field": "meaning", "label": "Что означает"},
                {"field": "quality", "label": "Качество"},
                {"field": "problem", "label": "Проблема"},
                {"field": "reward_ready", "label": "Готов для выплат"},
            ],
        ),
        table(
            "plan",
            "Точный порядок запуска",
            "plan_rows",
            plan_source,
            "order",
            [
                {"field": "order", "label": "#", "format": "number"},
                {"field": "when", "label": "Когда"},
                {"field": "action", "label": "Действие"},
                {"field": "gate", "label": "Ограничитель"},
            ],
        ),
    ]
    blocks = [
        {"id": "title", "type": "markdown", "body": f"# {TITLE}"},
        {
            "id": "executive",
            "type": "markdown",
            "body": (
                "## Executive Summary\n\n"
                "**Платить за первую подтверждённую оплату.** На 600 зрелых ordinary-referral регистрациях "
                "100 ₽ за paid стоили бы 21 300 ₽, а 100 ₽ за широкий activation proxy — 44 500 ₽. "
                "Paid-trigger оставляет 137,6 ₽ D30 выручки после награды на регистрацию; activation-trigger — 98,9 ₽. "
                "Ранний trigger должен создать **минимум на 39,1% больше регистраций**, просто чтобы сравняться по деньгам.\n\n"
                "При этом 100 ₽ за first paid почти не дороже текущих 20%: 21 300 ₽ против 19 645 ₽, всего **+1 655 ₽ (+8,4%)** "
                "на всей зрелой выборке. В июньской когорте полный rollout стоил бы лишь на 510 ₽ больше текущей схемы."
            ),
        },
        {
            "id": "definitions",
            "type": "markdown",
            "body": (
                "## Что именно посчитано\n\n"
                "Когорты: новые обычные рефералы с 13 апреля по 30 июня 2026 года; special partners исключены. "
                "Каждому дано 30 дней на оплату. `trial_used` — только широкий proxy активации, а не доказанное VPN-подключение. "
                "Поэтому это надёжная оценка верхней границы расходов раннего trigger, но не точная connect-конверсия."
            ),
        },
        {"id": "cohorts_block", "type": "table", "tableId": "cohorts"},
        {
            "id": "funnel",
            "type": "markdown",
            "body": (
                "## Воронка\n\n"
                "Из 600 регистраций 445 активировали trial/proxy (**74,2%**), 213 заплатили за 30 дней (**35,5%**). "
                "Из активировавших заплатили 156 (**35,1%**). То есть раннее событие происходит примерно в 2,09 раза чаще оплаты, "
                "а его аудитория не показала лучшей конверсии в деньги: часть клиентов покупает сразу, минуя trial."
            ),
        },
        {"id": "chart_block", "type": "chart", "chartId": "reward_cost_per_registration"},
        {"id": "models_block", "type": "table", "tableId": "models"},
        {
            "id": "actual",
            "type": "markdown",
            "body": (
                "## Почему фиксированные 100 ₽ за paid почти бесплатны относительно текущей схемы\n\n"
                "Средняя первая оплата в выборке — 461,2 ₽, поэтому текущие 20% уже стоят в среднем 92,2 ₽ на плательщика. "
                "Замена первой комиссии на фиксированные 100 ₽ добавляет всего 7,8 ₽ на плательщика. "
                "Правило `max(100 ₽, 20%)` сохраняет высокий бонус длинным тарифам, но стоит уже 26 701 ₽ — на 36% дороже control. "
                "При дефиците денег рекомендую именно **фиксированные 100 ₽ для новых referrals**, старые связи не менять."
            ),
        },
        {"id": "first_payments_block", "type": "table", "tableId": "first_payments"},
        {
            "id": "sensitivity_md",
            "type": "markdown",
            "body": (
                "## Что должно произойти, чтобы connect-trigger оказался лучше\n\n"
                "Если реальный verified-connect rate равен 60%, ранняя награда должна дать минимум **+21,7% регистраций** против paid-trigger; "
                "при 50% — **+11,8%**; при наблюдаемом широком proxy 74,2% — **+39,1%**. "
                "У нас нет причинных данных, что формулировка «за подключение» даст такой прирост. Запускать её сразу — покупать неизвестный uplift за реальные бонусы."
            ),
        },
        {"id": "sensitivity_block", "type": "table", "tableId": "sensitivity"},
        {
            "id": "telemetry",
            "type": "markdown",
            "body": (
                "## Текущая база не умеет безопасно платить за подключение\n\n"
                "`guide_connected` — нажатие пользователем кнопки «готово»; `remnawave first_connected_at` — неполный текущий snapshot; "
                "`trial_used` — выдача trial. Ни один сигнал сейчас не является полным append-only ledger реального VPN-сеанса. "
                "Для connect-награды сначала нужен серверный event с минимальным трафиком/сессией, device binding и защитой от повторного аккаунта."
            ),
        },
        {"id": "quality_block", "type": "table", "tableId": "quality"},
        {
            "id": "offer",
            "type": "markdown",
            "body": (
                "## Рекомендуемый оффер\n\n"
                "> **100 ₽ за друга**\n"
                ">\n"
                "> Получишь 100 ₽ на баланс, когда друг впервые оплатит DoodleVPN.\n"
                ">\n"
                "> **3 друга = месяц VPN.**\n\n"
                "Награда становится доступной через 48 часов; refund её отменяет. С повторных оплат друга снова начисляются 20%. "
                "Не обещать «за подключение», если деньги фактически начисляются за оплату."
            ),
        },
        {
            "id": "test",
            "type": "markdown",
            "body": (
                "## Как проверить без риска\n\n"
                "Один 50/50 test среди новых ordinary referrals: control — текущие 20%; treatment — фиксированные 100 ₽ с первой оплаты, далее 20%. "
                "Primary metric — D30 contribution на eligible referrer, а не число регистраций. "
                "На июньском объёме половина treatment добавила бы лишь около **255 ₽** расходов относительно control до учёта uplift. "
                "После ≥30 treatment first-paid раскатить только при росте и paid/referrer, и contribution/referrer."
            ),
        },
        {"id": "plan_block", "type": "table", "tableId": "plan"},
        {
            "id": "caveats",
            "type": "markdown",
            "body": (
                "## Caveats\n\n"
                "Модель вычитает награды из фактической D30 выручки; общие серверные и платёжные расходы, refunds и последующие продления "
                "не включены одинаково для сравниваемых вариантов. Исторической точной connect-конверсии нет, поэтому connect sensitivity показана сценарно. "
                "Вывод относится к ordinary referrals и не переносится на special partners."
            ),
        },
    ]
    artifact = {
        "surface": "report",
        "manifest": {
            "version": 1,
            "surface": "report",
            "title": TITLE,
            "description": "Production-data comparison of referral rewards triggered by VPN activation versus first successful payment.",
            "generatedAt": "2026-07-31T15:30:00+03:00",
            "sources": sources,
            "charts": [chart],
            "tables": tables,
            "blocks": blocks,
        },
        "snapshot": {
            "version": 1,
            "generatedAt": "2026-07-31T15:30:00+03:00",
            "status": "ready",
            "datasets": {
                "cohort_rows": cohort_rows,
                "first_payment_rows": first_payment_rows,
                "model_rows": model_rows,
                "connection_sensitivity_rows": connection_sensitivity_rows,
                "quality_rows": quality_rows,
                "plan_rows": plan_rows,
            },
        },
        "sources": sources,
    }
    (OUT / "artifact.json").write_text(json.dumps(artifact, ensure_ascii=False, indent=2), encoding="utf-8")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    build_notebook()
    build_artifact()
    evidence = {
        "as_of": "2026-07-31T15:30:00+03:00",
        "scope": "ordinary referrals created 2026-04-13 through 2026-06-30, D30 window",
        "cohorts": cohort_rows,
        "first_payment_replay": first_payment_rows,
        "models": model_rows,
        "connection_sensitivity": connection_sensitivity_rows,
        "data_quality": quality_rows,
        "decision": "fixed 100 RUB after first successful payment and hold; not on connection",
    }
    (OUT / "evidence.json").write_text(json.dumps(evidence, ensure_ascii=False, indent=2), encoding="utf-8")
    print(OUT / "doodlevpn_referral_trigger_analysis.ipynb")
    print(OUT / "artifact.json")


if __name__ == "__main__":
    main()
