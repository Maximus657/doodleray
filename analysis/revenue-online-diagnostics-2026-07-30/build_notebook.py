import ast
import contextlib
import io
import json
from pathlib import Path


OUT = Path(__file__).with_name("revenue_online_diagnostics.ipynb")


def md(text: str):
    return {"cell_type": "markdown", "metadata": {}, "source": text.strip()}


def code(text: str):
    return {
        "cell_type": "code",
        "execution_count": None,
        "metadata": {},
        "outputs": [],
        "source": text.strip(),
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
        """
# DoodleVPN: почему доход падает при росте онлайна

Срез на **2026-07-30**, бизнес-период начинается строго **2026-04-13**. Ноутбук содержит только агрегированные данные и воспроизводит ключевые расчёты отчёта.

**Основные источники**

- `/opt/accounting/data/accounting.db`: `revenue`, `revenue_refunds` — доход после комиссии и возвратов.
- `/opt/accounting/data/bot_mirror.db`: `users`, `platega_payments`, `crypto_payments`, `payment_url_clicks`, `start_attributions`, `broadcasts` — регистрации, trial, платёжная воронка и атрибуция.
- Remnawave PostgreSQL: `public.nodes_user_usage_history`, `public.users` — DAU/MAU и трафик.

Все даты текущего месяца частичные: июль — по 30 июля. История Remnawave usage начинается 17 апреля, поэтому сравнение онлайна ведётся с мая.
"""
    ),
    code(
        """
import math
import pandas as pd

pd.set_option("display.max_columns", 50)
pd.set_option("display.float_format", lambda x: f"{x:,.2f}")
"""
    ),
    md(
        """
## 1. Месячная экономика и онлайн

`net_revenue_usd` = сумма `revenue.amount_usd_net` минус связанные возвраты. Разложение new/returning использует доход после комиссии до возвратов (возвраты за весь период — только $18.77 и не меняют вывод).
"""
    ),
    code(
        """
monthly = pd.DataFrame([
    {"month":"2026-05", "net_revenue_usd":3581.06, "transactions":730, "payers":644, "aov_usd":4.91,
     "registrations":892, "new_payers":525, "new_revenue_usd":3009.63, "returning_payers":119, "returning_revenue_usd":574.73,
     "all_mau":1564, "tg_mau":1552, "restored_mau":4, "all_dau":823.9, "tg_dau":820.7, "restored_dau":0.3,
     "traffic_gb":91314.1, "tg_traffic_gb":87091.0, "restored_traffic_gb":7.2},
    {"month":"2026-06", "net_revenue_usd":3066.94, "transactions":642, "payers":576, "aov_usd":4.78,
     "registrations":627, "new_payers":257, "new_revenue_usd":1662.92, "returning_payers":319, "returning_revenue_usd":1404.02,
     "all_mau":1674, "tg_mau":1470, "restored_mau":201, "all_dau":992.1, "tg_dau":966.6, "restored_dau":24.6,
     "traffic_gb":134404.6, "tg_traffic_gb":133146.4, "restored_traffic_gb":1175.1},
    {"month":"2026-07", "net_revenue_usd":2816.42, "transactions":566, "payers":513, "aov_usd":4.98,
     "registrations":419, "new_payers":210, "new_revenue_usd":1467.50, "returning_payers":303, "returning_revenue_usd":1364.39,
     "all_mau":1921, "tg_mau":1450, "restored_mau":468, "all_dau":1150.5, "tg_dau":1061.8, "restored_dau":87.5,
     "traffic_gb":181392.0, "tg_traffic_gb":174358.4, "restored_traffic_gb":5788.9},
])
monthly["tx_per_payer"] = monthly.transactions / monthly.payers
monthly["revenue_per_active_day"] = monthly.net_revenue_usd / (monthly.all_dau * [31, 30, 30])
monthly["revenue_per_gb"] = monthly.net_revenue_usd / monthly.traffic_gb
monthly
"""
    ),
    code(
        """
def pct_change(a, b):
    return 100 * (b / a - 1)

may, jun, jul = (monthly.iloc[i] for i in range(3))
headline = pd.Series({
    "revenue May→Jul": pct_change(may.net_revenue_usd, jul.net_revenue_usd),
    "payers May→Jul": pct_change(may.payers, jul.payers),
    "AOV May→Jul": pct_change(may.aov_usd, jul.aov_usd),
    "all MAU May→Jul": pct_change(may.all_mau, jul.all_mau),
    "core tg MAU May→Jul": pct_change(may.tg_mau, jul.tg_mau),
    "core tg DAU May→Jul": pct_change(may.tg_dau, jul.tg_dau),
    "core tg traffic May→Jul": pct_change(may.tg_traffic_gb, jul.tg_traffic_gb),
    "revenue / active-day May→Jul": pct_change(may.revenue_per_active_day, jul.revenue_per_active_day),
    "revenue / GB May→Jul": pct_change(may.revenue_per_gb, jul.revenue_per_gb),
}).round(1)
headline.to_frame("change_pct")
"""
    ),
    md(
        """
### Разложение падения дохода

Тождество: `revenue = unique payers × transactions per payer × average transaction value`. Последовательное разложение сохраняет точную сумму изменения.
"""
    ),
    code(
        """
def decompose(start, end):
    p0, f0, a0 = start.payers, start.tx_per_payer, start.net_revenue_usd / start.transactions
    p1, f1, a1 = end.payers, end.tx_per_payer, end.net_revenue_usd / end.transactions
    return pd.Series({
        "payer_count_effect": (p1 - p0) * f0 * a0,
        "frequency_effect": p1 * (f1 - f0) * a0,
        "aov_effect": p1 * f1 * (a1 - a0),
        "total_change": end.net_revenue_usd - start.net_revenue_usd,
    }).round(2)

decomposition = pd.DataFrame({
    "May→Jun": decompose(may, jun),
    "Jun→Jul": decompose(jun, jul),
    "May→Jul": decompose(may, jul),
})
decomposition
"""
    ),
    md(
        """
## 2. Почему «онлайн растёт» — неправильная бизнес-интерпретация

В Remnawave появился растущий технический сегмент `wl_restor*`: 4 MAU в мае, 201 в июне, 468 в июле. Ядро `tg_*` по MAU за это время снизилось. При этом DAU ядра и трафик выросли: меньше уникальных Telegram-аккаунтов используют сервис интенсивнее и/или имеют уже оплаченные длинные подписки. Онлайн — метрика потребления накопленного subscriber stock, cash revenue — поток новых оплат за месяц.
"""
    ),
    code(
        """
online_bridge = pd.DataFrame([
    {"metric":"Total MAU", "May":1564, "June":1674, "July":1921},
    {"metric":"Core tg_* MAU", "May":1552, "June":1470, "July":1450},
    {"metric":"Restored wl_restor* MAU", "May":4, "June":201, "July":468},
    {"metric":"Core tg_* DAU", "May":820.7, "June":966.6, "July":1061.8},
    {"metric":"Core tg_* traffic, GB", "May":87091.0, "June":133146.4, "July":174358.4},
])
online_bridge
"""
    ),
    md(
        """
## 3. Воронка новых пользователей

Для честного сравнения июнь и июль ограничены одинаковыми окнами **1–23 число**; оплата считается в течение 7 дней после регистрации.
"""
    ),
    code(
        """
funnel = pd.DataFrame([
    {"cohort":"2026-06-01..23", "registered":525, "trial_started":377, "trial_rate_pct":71.8, "paid_7d":116, "paid_7d_pct":22.1, "trial_to_paid_7d_pct":22.5},
    {"cohort":"2026-07-01..23", "registered":304, "trial_started":199, "trial_rate_pct":65.5, "paid_7d":62, "paid_7d_pct":20.4, "trial_to_paid_7d_pct":20.6},
])
funnel
"""
    ),
    code(
        """
jun_f, jul_f = funnel.iloc[0], funnel.iloc[1]
expected_july_at_june_volume = jun_f.registered * jul_f.paid_7d_pct / 100
volume_loss = expected_july_at_june_volume - jul_f.paid_7d
conversion_loss = jun_f.registered * (jun_f.paid_7d_pct - jul_f.paid_7d_pct) / 100
pd.Series({
    "registrations_change_pct": pct_change(jun_f.registered, jul_f.registered),
    "paid_7d_change_pct": pct_change(jun_f.paid_7d, jul_f.paid_7d),
    "conversion_change_pp": jul_f.paid_7d_pct - jun_f.paid_7d_pct,
    "estimated_share_of_paid7_decline_from_volume_pct": 100 * volume_loss / (volume_loss + conversion_loss),
}).round(1).to_frame("value")
"""
    ),
    md(
        """
## 4. Тарифы, удержание и платёжные провайдеры

- Возвратный доход почти не изменился между июнем и июлем: $1,404 → $1,364.
- 1-месячное продление в окне 21–45 дней снизилось умеренно: 65.2% → 59.8% для сопоставимых когорт первой половины мая/июня.
- Wata не выглядит текущим узким местом: clicked→paid вырос с 77.8% до 82.7%.
- Крипто-воронка требует отдельной проверки: settled/attempt упал с 56.0% в мае до ~32% в июне-июле; среди тех, кто нажал платёжную ссылку, clicked→paid стабилен около 57–60%, значит основной провал раньше или в неполной click-телеметрии.
"""
    ),
    code(
        """
plans = pd.DataFrame([
    ["2026-05","1m",387,1194.53],["2026-05","3m",148,1020.92],["2026-05","6m",60,716.10],["2026-05","1y",30,520.14],
    ["2026-06","1m",368,1223.92],["2026-06","3m",108,832.36],["2026-06","6m",38,471.77],["2026-06","1y",18,407.11],
    ["2026-07","1m",290,908.78],["2026-07","3m",140,1026.81],["2026-07","6m",53,635.28],["2026-07","1y",10,183.10],
], columns=["month","plan","transactions","net_revenue_usd"])
plans.pivot(index="plan", columns="month", values=["transactions","net_revenue_usd"])
"""
    ),
    code(
        """
payments = pd.DataFrame([
    ["Platega","2026-05",975,629,64.5,None],
    ["Platega","2026-06",593,325,54.8,None],
    ["Wata","2026-06",394,260,66.0,77.8],
    ["Wata","2026-07 through 23",576,393,68.2,82.7],
    ["Crypto","2026-05",141,79,56.0,None],
    ["Crypto","2026-06",112,36,32.1,56.8],
    ["Crypto","2026-07 through 23",67,21,31.3,60.0],
], columns=["provider","period","attempts","settled","settled_pct","clicked_to_paid_pct"])
payments
"""
    ),
    md(
        """
## 5. Контрольные проверки

Проверки специально падают, если ключевые агрегаты или интерпретации случайно меняются при редактировании ноутбука.
"""
    ),
    code(
        """
assert math.isclose(monthly.net_revenue_usd.sum(), 9464.42, abs_tol=0.01)
assert jul.net_revenue_usd < jun.net_revenue_usd < may.net_revenue_usd
assert jul.payers < jun.payers < may.payers
assert jul.aov_usd > may.aov_usd
assert jul.all_mau > may.all_mau and jul.tg_mau < may.tg_mau
assert payments.query("provider == 'Wata'").iloc[1].clicked_to_paid_pct > payments.query("provider == 'Wata'").iloc[0].clicked_to_paid_pct
assert funnel.iloc[1].paid_7d_pct >= 20
print("All checks passed")
"""
    ),
    md(
        """
## Вывод

Падение дохода — в первую очередь **провал объёма новых пользователей и новых плательщиков**, начавшийся примерно с недели 15 июня. Цена/AOV и основной Wata-флоу не объясняют падение. Рост «онлайна» смешивает технически восстановленные аккаунты с ядром и отражает растущую интенсивность уже оплаченного использования, поэтому не должен использоваться как прокси дохода.
"""
    ),
]

def execute_notebook(notebook: dict) -> None:
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
                    exec(compile(ast.fix_missing_locations(prefix), f"cell-{execution_count}", "exec"), namespace)
                result = eval(compile(ast.Expression(tree.body[-1].value), f"cell-{execution_count}", "eval"), namespace)
            else:
                exec(compile(tree, f"cell-{execution_count}", "exec"), namespace)
        outputs = []
        if stdout.getvalue():
            outputs.append({"name": "stdout", "output_type": "stream", "text": stdout.getvalue()})
        if result is not None:
            outputs.append({
                "data": {"text/plain": repr(result)},
                "execution_count": execution_count,
                "metadata": {},
                "output_type": "execute_result",
            })
        cell["outputs"] = outputs


for index, cell in enumerate(nb["cells"], 1):
    cell["id"] = f"cell-{index:02d}"

execute_notebook(nb)
OUT.write_text(json.dumps(nb, ensure_ascii=False, indent=1), encoding="utf-8")
print(OUT)
