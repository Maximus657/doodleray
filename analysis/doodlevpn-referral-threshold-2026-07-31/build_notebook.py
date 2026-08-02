from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path


OUT = Path(__file__).resolve().parent


gift_windows = [
    {"event": "+3 дня, май", "phase": "До", "phase_order": 1, "days": 3, "recipients": 5179, "payments": 41, "first": 16, "repeat": 25},
    {"event": "+3 дня, май", "phase": "Подарок", "phase_order": 2, "days": 3, "recipients": 5179, "payments": 27, "first": 13, "repeat": 14},
    {"event": "+3 дня, май", "phase": "После", "phase_order": 3, "days": 3, "recipients": 5179, "payments": 48, "first": 32, "repeat": 16},
    {"event": "+7 дней, июль", "phase": "До", "phase_order": 1, "days": 7, "recipients": 4707, "payments": 147, "first": 29, "repeat": 118},
    {"event": "+7 дней, июль", "phase": "Подарок", "phase_order": 2, "days": 7, "recipients": 4707, "payments": 56, "first": 19, "repeat": 37},
    {"event": "+7 дней, июль", "phase": "После", "phase_order": 3, "days": 7, "recipients": 4707, "payments": 150, "first": 47, "repeat": 103},
]

monthly_thresholds = [
    {"month": "Апрель с 13-го", "paid_friends": 66, "active_referrers": 59, "reward_1": 59, "reward_2": 6, "reward_3": 1},
    {"month": "Май", "paid_friends": 94, "active_referrers": 76, "reward_1": 76, "reward_2": 12, "reward_3": 4},
    {"month": "Июнь", "paid_friends": 105, "active_referrers": 85, "reward_1": 85, "reward_2": 15, "reward_3": 4},
    {"month": "Июль", "paid_friends": 57, "active_referrers": 51, "reward_1": 51, "reward_2": 6, "reward_3": 0},
]

variants = [
    {"variant": "30 дней за 1 оплатившего друга", "july_reward_users": 51, "service_months": 51.0, "push": "очень сильный", "cashflow_risk": "очень высокий", "decision": "не запускать"},
    {"variant": "30 дней за 2 оплативших друзей", "july_reward_users": 6, "service_months": 6.0, "push": "сильный", "cashflow_risk": "низкий и ограничиваемый", "decision": "рекомендация"},
    {"variant": "30 дней за 3 оплативших друзей", "july_reward_users": 0, "service_months": 0.0, "push": "слабый из-за недостижимости", "cashflow_risk": "низкий", "decision": "не запускать первым"},
    {"variant": "14 дней за каждого оплатившего", "july_reward_users": 51, "service_months": 26.6, "push": "средний", "cashflow_risk": "средний", "decision": "резерв"},
    {"variant": "7 дней за каждого оплатившего", "july_reward_users": 51, "service_months": 13.3, "push": "слабый-средний", "cashflow_risk": "средний", "decision": "проигрывает порогу 2"},
    {"variant": "+7 дней приглашённому после оплаты", "july_reward_users": 57, "service_months": 13.3, "push": "средний", "cashflow_risk": "прямо откладывает его продление", "decision": "не запускать"},
    {"variant": "Только текущие 20%", "july_reward_users": 0, "service_months": 0.0, "push": "слабый", "cashflow_risk": "низкий", "decision": "оставить как основу, но не как толчок"},
    {"variant": "Лотерея / кейсы", "july_reward_users": 0, "service_months": 0.0, "push": "неизвестный", "cashflow_risk": "непредсказуемый + fraud", "decision": "не сейчас"},
]


def pct_change(current: float, baseline: float) -> float:
    return (current / baseline - 1) * 100


def build_cells() -> list[dict]:
    gift_json = json.dumps(gift_windows, ensure_ascii=False)
    month_json = json.dumps(monthly_thresholds, ensure_ascii=False)
    variant_json = json.dumps(variants, ensure_ascii=False)
    return [
        md("## tl;dr\n\nМассовые бесплатные дни уже дважды совпали с резким провалом оплат. Рекомендация: не давать месяц за одного друга и не давать дополнительные дни приглашённому. Запустить ограниченный 30-дневный тест **«2 новых оплативших друга → 30 дней рефереру»** поверх текущих 20%, только для новых событий и с контрольной группой."),
        md("## Context & Methods\n\n### Key Assumptions\n\n- Источник: read-only production SQLite `/opt/doodlevpn-data/bot.db`, срез 31 июля 2026 года.\n- Успешная оплата дедуплицирована по provider payment row; Stars берутся из канонического ledger.\n- Майская раздача восстановлена из `mass_grant_runs/results`; июльская — из `broadcasts/broadcast_jobs` и платёжной временной серии.\n- Это естественные эксперименты без holdout: техработы, сезонность и июльская скидка 37% мешают причинной оценке. Поэтому цифры используются как риск-сигнал, не как точный causal lift."),
        code(f"gift_windows = {gift_json}\nfor event in sorted(set(r['event'] for r in gift_windows)):\n    rows = sorted((r for r in gift_windows if r['event'] == event), key=lambda r: r['phase_order'])\n    before, gift, after = rows\n    print(event, 'payment change during gift:', round((gift['payments']/before['payments']-1)*100, 1), '%', 'repeat change:', round((gift['repeat']/before['repeat']-1)*100, 1), '%')"),
        md("## Data\n\nРавные по длине окна сравниваются вокруг момента начисления. В июле окно после подарка содержит 44 оплаты со скидкой 37%, поэтому восстановление числа транзакций завышает восстановление выручки."),
        code(f"monthly_thresholds = {month_json}\nfor r in monthly_thresholds:\n    print(r['month'], 'paid friends=', r['paid_friends'], 'reward users at thresholds 1/2/3=', r['reward_1'], r['reward_2'], r['reward_3'])"),
        md("## Results\n\nВ июле за точные семь дней до раздачи было 147 оплат (118 повторных), во время подарка — 56 (37 повторных), после — 150 (103 повторных). За первые 23,1 часа после окончания подарка, ещё до массовой рассылки скидки, прошло 30 оплат против 56 за все 168 часов подарочного окна.\n\nВ мае эффект слабее, но направлен так же: повторные оплаты 25 → 14 в трёхдневное окно (−44%)."),
        code(f"variants = {variant_json}\nfor r in variants:\n    print(f\"{{r['variant']}} | July service-months={{r['service_months']}} | {{r['decision']}}\")"),
        md("## Takeaways\n\n- Месяц за одного друга охватил бы 51 июльского реферера и создал бы 51 бесплатный пользователь-месяц на базовом объёме — до доказанного прироста.\n- Порог три в июле не достиг никто; такой оффер почти не создаёт обратной связи.\n- Порог два дал бы только 6 бесплатных месяцев на базовом июльском потоке, но сохраняет сильный заголовок и понятный прогресс 0/2 → 1/2 → 2/2.\n- Тест должен считать только первые оплаты прямых друзей после старта, исключать специальных партнёров и выдавать награду после 48-часового hold.\n- Масштабирование допустимо только по incremental paid referrals и incremental contribution относительно контроля."),
    ]


def md(source: str) -> dict:
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
        stream = io.StringIO()
        source = "".join(cell["source"])
        with contextlib.redirect_stdout(stream):
            exec(compile(source, f"cell-{count}", "exec"), scope)
        cell["execution_count"] = count
        cell["outputs"] = [{"name": "stdout", "output_type": "stream", "text": stream.getvalue().splitlines(keepends=True)}]


def main() -> None:
    cells = build_cells()
    execute(cells)
    notebook = {
        "cells": cells,
        "metadata": {"kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"}, "language_info": {"name": "python", "version": "3"}},
        "nbformat": 4,
        "nbformat_minor": 5,
    }
    (OUT / "doodlevpn_referral_threshold.ipynb").write_text(json.dumps(notebook, ensure_ascii=False, indent=1), encoding="utf-8")
    evidence = {
        "as_of": "2026-07-31T00:00:00+03:00",
        "gift_windows": gift_windows,
        "monthly_thresholds": monthly_thresholds,
        "variants": variants,
        "source_tables": ["payments", "platega_payments", "crypto_payments", "mass_grant_runs", "mass_grant_results", "broadcasts", "broadcast_jobs", "user_events", "users"],
        "validation": {"july_payment_drop_pct": round(pct_change(56, 147), 1), "july_repeat_drop_pct": round(pct_change(37, 118), 1), "may_repeat_drop_pct": round(pct_change(14, 25), 1)},
    }
    (OUT / "evidence.json").write_text(json.dumps(evidence, ensure_ascii=False, indent=2), encoding="utf-8")
    print("built and executed", OUT / "doodlevpn_referral_threshold.ipynb")


if __name__ == "__main__":
    main()
