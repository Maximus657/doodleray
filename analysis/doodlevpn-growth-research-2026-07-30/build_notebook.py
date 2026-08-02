import ast
import contextlib
import io
import json
import math
from pathlib import Path

import pandas as pd


ROOT = Path(__file__).parent
OUT = ROOT / "doodlevpn_growth_research.ipynb"


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
# DoodleVPN: независимая growth-диагностика и модель решений

Срез закрыт на **30 июля 2026, 23:59:59 МСК**. Потоки продукта считаются строго с **13 апреля 2026**; lifetime-метрики используются только там, где они явно так названы. Ноутбук не содержит персональных данных или доступов.

**Решение:** сначала сделать реферальное вознаграждение ликвидным внутри VPN и дать другу реальную выгоду после первой оплаты; игру проверять только отдельным вторым слоем. Главная доказанная причина падения — меньше новых плательщиков, особенно из просевших специальных партнёров и из-за сокращения числа активных обычных рефереров.
"""
    ),
    code(
        """
import pandas as pd
from statistics import NormalDist

pd.set_option("display.max_columns", 100)
pd.set_option("display.width", 160)
pd.set_option("display.float_format", lambda x: f"{x:,.2f}")
"""
    ),
    md(
        """
## 1. Источники и определения

- `accounting.db.revenue` минус `revenue_refunds`: чистая выручка после платёжных комиссий и возвратов.
- `bot_mirror.db.users`, `payments`, `referral_holds`, `withdrawal_requests`, `start_attributions`, `user_events`: регистрации, оплаты, рефералы, балансы и сообщения.
- Remnawave PostgreSQL `public.users`, `public.nodes_user_usage_history`: активные аккаунты и потребление.
- Новый плательщик — первая наблюдаемая оплата после 13 апреля. Поэтому пользователь, плативший до начала окна, теоретически может быть ошибочно классифицирован как новый.
- Обычный реферер — текущая ставка 20%; специальный партнёр — ставка не 20%.
- Июль закрыт по UTC `2026-07-30 21:00:00`. После исходного снимка владельца прошла поздняя оплата $7.52, поэтому полный день отличается от цифр в промпте на одну оплату и одного плательщика.
"""
    ),
    code(
        """
monthly = pd.DataFrame([
    {"month":"Apr 13–30", "revenue":1172.92, "transactions":261, "payers":248, "registrations":691},
    {"month":"May", "revenue":3581.06, "transactions":730, "payers":644, "registrations":892},
    {"month":"June", "revenue":3066.94, "transactions":642, "payers":576, "registrations":627},
    {"month":"July 1–30", "revenue":2823.94, "transactions":567, "payers":514, "registrations":420},
])
monthly["aov"] = monthly.revenue / monthly.transactions
monthly
"""
    ),
    code(
        """
new_returning = pd.DataFrame([
    {"month":"May", "new_revenue":3006.33, "returning_revenue":574.73},
    {"month":"June", "new_revenue":1662.92, "returning_revenue":1404.02},
    {"month":"July 1–30", "new_revenue":1452.03, "returning_revenue":1371.91},
])
new_returning["total"] = new_returning.new_revenue + new_returning.returning_revenue
new_returning
"""
    ),
    md(
        """
## 2. Root-cause bridge

Май → июль: выручка снизилась на **$757.12 (−21.1%)**, при стабильном среднем платеже около $5. Возвратная выручка июня и июля почти одинакова; основной провал — приток новых оплат. По текущей атрибуции 55.5% денежного снижения объясняет специальный партнёрский сегмент, ещё 30.1% — обычный 20%-й реферальный сегмент. Нереферальный сегмент объясняет лишь 14.5%, но его регистрационная атрибуция ненадёжна.
"""
    ),
    code(
        """
segments = pd.DataFrame([
    {"segment":"Non-ref / unattributed", "may":1471.30, "july":1361.67},
    {"segment":"Retail referral, 20%", "may":1203.32, "july":975.75},
    {"segment":"Special partners", "may":906.44, "july":486.52},
])
segments["change"] = segments.july - segments.may
segments["share_of_decline"] = -segments.change / -segments.change.sum()
segments
"""
    ),
    code(
        """
retail_activity = pd.DataFrame([
    {"month":"Apr 13–30", "ref_registrations":142, "active_referrers":96, "regs_per_referrer":1.48},
    {"month":"May", "ref_registrations":250, "active_referrers":163, "regs_per_referrer":1.53},
    {"month":"June", "ref_registrations":208, "active_referrers":135, "regs_per_referrer":1.54},
    {"month":"July 1–30", "ref_registrations":102, "active_referrers":83, "regs_per_referrer":1.23},
])
may = retail_activity.iloc[1]
july = retail_activity.iloc[3]
count_effect = (july.active_referrers - may.active_referrers) * may.regs_per_referrer
productivity_effect = july.active_referrers * (july.regs_per_referrer - may.regs_per_referrer)
retail_activity, pd.Series({"count_effect":count_effect, "productivity_effect":productivity_effect})
"""
    ),
    md(
        """
**Интерпретация:** примерно 83% падения обычных реферальных регистраций связано с тем, что реферить стало меньше людей; около 17% — с меньшей продуктивностью оставшихся. Это проблема активации/видимости/оффера, а не доказательство, что 20% как тип награды фундаментально не работает.
"""
    ),
    code(
        """
referrer_distribution = pd.DataFrame([
    {"window":"Strict Apr 13+", "referrers":414, "one":253, "two":70, "three":37, "four_to_nine":41, "ten_plus":13},
    {"window":"Lifetime context", "referrers":609, "one":338, "two":103, "three":56, "four_to_nine":79, "ten_plus":33},
])
referrer_distribution["share_stopped_1_2"] = (referrer_distribution.one + referrer_distribution.two) / referrer_distribution.referrers
referrer_distribution
"""
    ),
    md(
        """
Строгий срез хуже lifetime-картины: **78.0%** рефереров после 13 апреля остановились на одном-двух зарегистрированных друзьях. Поэтому порог «три активных платных друга» скрывает ценность от большинства и не должен быть стартовой гипотезой.
"""
    ),
    code(
        """
active_payer_stock = pd.DataFrame([
    {"active_paid_referrals":"0", "active_payers":960},
    {"active_paid_referrals":"1", "active_payers":116},
    {"active_paid_referrals":"2", "active_payers":36},
    {"active_paid_referrals":"3", "active_payers":14},
    {"active_paid_referrals":"4+", "active_payers":18},
])
active_payer_stock
"""
    ),
    md(
        """
## 3. Data quality: что известно и чего нет

- `campaigns`, `campaign_links`, `campaign_user_attribution`, `campaign_conversions` пусты.
- У 73.5% июльских нереферальных регистраций источник не заполнен; `nonref` нельзя считать настоящим direct/organic.
- `start_attributions` и `user_events` начинаются 8 июня, а `trial_started_at` неполон в мае.
- История сообщений частично изменчива и пересекается с broadcast-логами. Стабильно воспроизводится только ежедневная pretrial-цепочка: 6,875 отправок 365 людям, в среднем 18.8 сообщения; после первого сообщения когда-либо оплатили 7, в первые сутки — 0, за 7 дней — 5. Holdout отсутствует.
- Канонического определения renewal и eligibility нет; сильный вывод о падении retention пока делать нельзя.
"""
    ),
    md(
        """
## 4. Юнит-экономика вариантов

Модель ниже — **не прогноз**, а чувствительность. База июля: 35 обычных платных рефералов в месяц, 30-дневная выручка такого плательщика $6.98, текущая комиссия 20% = $1.40. Инфраструктура: $0.24 pessimistic / $0.12 base / $0.05 aggressive на пользователь-месяц. Возможная потеря выручки от бесплатных дней показывается отдельно от серверной стоимости.
"""
    ),
    code(
        """
SCENARIOS = {
    "pessimistic": {"infra":0.24, "opp_factor":1.00, "uplift":"low"},
    "base": {"infra":0.12, "opp_factor":0.50, "uplift":"base"},
    "aggressive": {"infra":0.05, "opp_factor":0.25, "uplift":"high"},
}

strategies = pd.DataFrame([
    {"id":"current_20", "label":"Текущие 20%", "source":"retail", "low":0, "base":0, "high":0, "commission":.20},
    {"id":"payout_3", "label":"20%, вывод от $3", "source":"retail", "low":.05, "base":.12, "high":.20, "commission":.20, "cash_release":[50,150,300]},
    {"id":"internal_balance", "label":"20% + мгновенная трата в VPN", "source":"retail", "low":.10, "base":.25, "high":.40, "commission":.20, "cannibalized":[10,25,50]},
    {"id":"free_days", "label":"7 дней рефереру вместо денег", "source":"retail", "low":.08, "base":.20, "high":.35, "commission":0, "days_referrer":7},
    {"id":"threshold_free", "label":"3 активных = бесплатный VPN", "source":"retail", "low":.05, "base":.15, "high":.30, "commission":.20, "cannibalized":[36.3,36.3,36.3]},
    {"id":"choice", "label":"Выбор: деньги или VPN", "source":"retail", "low":.10, "base":.20, "high":.35, "commission":.20, "cannibalized":[8,20,40]},
    {"id":"two_sided", "label":"20% + 7 дней другу после оплаты", "source":"retail", "low":.12, "base":.28, "high":.50, "commission":.20, "days_friend":7},
    {"id":"sprint", "label":"30-дневный referral-спринт", "source":"retail", "low":.15, "base":.35, "high":.60, "commission":.20, "days_friend":7, "days_referrer":7, "fixed_prize":[10,20,30]},
    {"id":"lottery", "label":"Розыгрыш среди оплативших друзей", "source":"retail", "low":.12, "base":.30, "high":.60, "commission":.20, "fixed_prize":[50,75,100], "fraud":.08},
    {"id":"game_store", "label":"Игра + фиксированный магазин", "source":"retail", "low":.10, "base":.30, "high":.60, "commission":.20, "cash_reward":.40, "fixed_prize":[0,25,50], "fraud":.05},
    {"id":"game_random", "label":"Игра + случайные ценные призы", "source":"retail", "low":.15, "base":.40, "high":.80, "commission":.20, "cash_reward":.50, "fixed_prize":[75,100,150], "fraud":.12},
    {"id":"partner_new", "label":"Новые партнёры: $2 first-paid + 10% recurring", "source":"special", "direct":[1,4,8], "commission":.10, "cash_reward":2.00, "fraud":.05},
    {"id":"lifecycle", "label":"Lifecycle/winback без скидки", "source":"returning", "direct":[4,10,18], "commission":0},
    {"id":"hybrid", "label":"Внутренний баланс + 7 дней другу", "source":"retail", "low":.18, "base":.40, "high":.70, "commission":.20, "days_friend":7, "cannibalized":[10,20,40], "fraud":.03},
    {"id":"hybrid_game", "label":"Гибрид + сезонный Doodle Route", "source":"retail", "low":.22, "base":.50, "high":.90, "commission":.20, "days_friend":7, "cash_reward":.25, "fixed_prize":[0,20,50], "cannibalized":[10,20,40], "fraud":.05},
]).fillna(0)

BASELINE_PAID = {"retail":35, "special":0, "returning":0}
REV_PER_PAYER = {"retail":6.98, "special":5.74, "returning":4.50}
ONE_MONTH_NET = 3.13

def value_for(row, field, scenario):
    value = row[field]
    if isinstance(value, list):
        return value[["pessimistic", "base", "aggressive"].index(scenario)]
    return float(value)

def evaluate(row, scenario):
    assumptions = SCENARIOS[scenario]
    if isinstance(row.direct, list):
        inc_payers = row.direct[["pessimistic", "base", "aggressive"].index(scenario)]
    else:
        inc_payers = BASELINE_PAID[row.source] * row[assumptions["uplift"]]
    rev_pp = REV_PER_PAYER[row.source]
    inc_revenue = inc_payers * rev_pp
    obligation = inc_revenue * row.commission
    baseline = BASELINE_PAID[row.source]
    reward_users = baseline + inc_payers if row.source == "retail" and (row.cash_reward or row.days_friend or row.days_referrer) else inc_payers
    cash_reward = reward_users * row.cash_reward
    free_days = row.days_friend + row.days_referrer
    free_infra = reward_users * assumptions["infra"] * free_days / 30
    opportunity = reward_users * ONE_MONTH_NET * free_days / 30 * assumptions["opp_factor"]
    server = inc_payers * assumptions["infra"]
    fixed_prize = value_for(row, "fixed_prize", scenario)
    cannibalized = value_for(row, "cannibalized", scenario)
    fraud = row.fraud * (obligation + cash_reward + fixed_prize)
    baseline_commission_savings = baseline * rev_pp * max(0, .20 - row.commission) if row.source == "retail" else 0
    contribution = inc_revenue - obligation - cash_reward - free_infra - opportunity - server - fixed_prize - cannibalized - fraud + baseline_commission_savings
    variable_margin = rev_pp * (1-row.commission) - row.cash_reward - assumptions["infra"] * (1 + free_days/30) - ONE_MONTH_NET * free_days/30 * assumptions["opp_factor"]
    baseline_subsidy = max(0, (cash_reward + free_infra + opportunity) - inc_payers * (row.cash_reward + assumptions["infra"] * free_days/30 + ONE_MONTH_NET * free_days/30 * assumptions["opp_factor"]))
    fixed = fixed_prize + cannibalized + baseline_subsidy - baseline_commission_savings
    break_even = fixed / variable_margin if variable_margin > 0 else math.inf
    max_reward = max(0, inc_revenue - obligation - server - 2 * inc_payers)
    return {
        "scenario":scenario, "strategy":row.label, "incremental_payers":inc_payers,
        "incremental_30d_revenue":inc_revenue, "existing_ref_obligation":obligation,
        "new_cash_rewards":cash_reward, "free_access_infra":free_infra,
        "server_cost":server, "external_prizes":fixed_prize,
        "opportunity_cost":opportunity, "cannibalized_revenue":cannibalized,
        "fraud_reserve":fraud, "baseline_commission_savings":baseline_commission_savings,
        "baseline_reward_subsidy":baseline_subsidy, "net_incremental_contribution":contribution,
        "break_even_incremental_payers":break_even, "max_additional_reward_budget_at_$2_margin":max_reward,
        "cash_release_existing_liability":value_for(row, "cash_release", scenario),
    }

unit = pd.DataFrame(evaluate(row, scenario) for _, row in strategies.iterrows() for scenario in SCENARIOS)
unit.query("scenario == 'base'").sort_values("net_incremental_contribution", ascending=False)
"""
    ),
    md(
        """
### Чтение модели

- `existing_ref_obligation` — 20% начисление на **новую** реферальную выручку; это не дополнительная награда.
- `cash_release_existing_liability` у сниженного порога — ускорение выплаты уже заработанного обязательства, а не расход периода.
- `opportunity_cost` бесплатных дней — риск отложенного продления; `free_access_infra` — физическая стоимость обслуживания. Они показаны отдельно.
- $308 / 1,282 активных = $0.24 — полностью распределённая, а не маржинальная себестоимость. Поэтому $0.12 base — рабочая гипотеза, не измеренный факт.
"""
    ),
    md(
        """
## 5. 100-балльная оценка стратегий

Оценки 0–100 — экспертные priors, а не данные. Итоговые веса заданы владельцем. Чувствительность проверяет profit-heavy, viral-heavy и risk/speed-heavy режимы.
"""
    ),
    code(
        """
score = pd.DataFrame([
    ["Текущие 20%",45,72,48,100,70,68,82,60,80],
    ["20%, вывод от $3",60,78,55,95,82,66,78,80,80],
    ["20% + мгновенная трата в VPN",78,92,64,92,95,84,90,95,92],
    ["7 дней рефереру вместо денег",62,86,68,90,92,70,82,90,90],
    ["3 активных = бесплатный VPN",54,82,60,86,72,58,68,90,84],
    ["Выбор: деньги или VPN",73,91,70,85,84,80,82,92,90],
    ["20% + 7 дней другу после оплаты",81,92,85,88,94,75,80,94,92],
    ["30-дневный referral-спринт",74,88,90,84,90,58,66,92,78],
    ["Розыгрыш среди оплативших друзей",42,68,82,74,76,42,35,86,30],
    ["Игра + фиксированный магазин",61,80,86,65,76,62,60,84,76],
    ["Игра + случайные ценные призы",45,72,91,62,78,46,35,80,42],
    ["Новые партнёры: $2 first-paid + 10% recurring",76,82,72,72,76,80,70,90,76],
    ["Lifecycle/winback без скидки",82,92,38,92,90,86,94,94,94],
    ["Внутренний баланс + 7 дней другу",86,95,88,78,88,86,78,95,92],
    ["Гибрид + сезонный Doodle Route",72,86,94,58,74,70,58,88,76],
], columns=["strategy","profit","ru_fit","viral","speed","clarity","durability","antifraud","measurement","reputation"])

weight_sets = {
    "owner": {"profit":25,"ru_fit":15,"viral":15,"speed":10,"clarity":10,"durability":10,"antifraud":5,"measurement":5,"reputation":5},
    "profit_heavy": {"profit":40,"ru_fit":10,"viral":10,"speed":10,"clarity":5,"durability":10,"antifraud":5,"measurement":5,"reputation":5},
    "viral_heavy": {"profit":20,"ru_fit":10,"viral":30,"speed":10,"clarity":5,"durability":10,"antifraud":5,"measurement":5,"reputation":5},
    "risk_speed": {"profit":20,"ru_fit":10,"viral":10,"speed":20,"clarity":10,"durability":5,"antifraud":10,"measurement":10,"reputation":5},
}
for name, weights in weight_sets.items():
    score[name] = sum(score[k] * v for k, v in weights.items()) / 100
score.sort_values("owner", ascending=False)
"""
    ),
    md(
        """
## 6. Почему обычный A/B слаб и что делать

Из 1,144 действующих плательщиков за 30 дней реферили 75 (6.6%). При такой базе классический двухсторонний тест имеет мощность только на очень крупный эффект. Поэтому тесты должны быть последовательными, с предварительно заданным бизнес-порогом и Bayesian/Poisson-моделью событий, а не с ожиданием `p<0.05` любой ценой.
"""
    ),
    code(
        """
def n_per_arm(p1, p2, alpha=.05, power=.80):
    za = NormalDist().inv_cdf(1-alpha/2)
    zb = NormalDist().inv_cdf(power)
    p = (p1+p2)/2
    return math.ceil(2 * p * (1-p) * (za+zb)**2 / (p2-p1)**2)

baseline_activation = 75/1144
power = pd.DataFrame([
    {"target_relative_lift":lift, "target_rate":baseline_activation*(1+lift), "n_per_arm":n_per_arm(baseline_activation, baseline_activation*(1+lift))}
    for lift in [.20,.30,.40,.50,.70,1.00]
])
power
"""
    ),
    md(
        """
## 7. Игровые концепции

Баллы 1–5. `MVP` означает реалистичность за 1–3 дня. Случайные денежные награды намеренно штрафуются за fraud/legal/reputation.
"""
    ),
    code(
        """
games = pd.DataFrame([
    ["Doodle Route: сезонная карта",5,5,4,5,4,5,4,4,4],
    ["Internet Passport: марки стран",5,4,4,4,4,5,5,4,4],
    ["Shield Team: команда из 3–5",4,5,4,5,4,4,3,4,4],
    ["Referral Relay: цепочка подарков",4,5,3,5,4,4,3,4,4],
    ["Shield City: общая защита города",4,4,4,4,3,3,4,4,4],
    ["Privacy Garden / питомец",5,3,4,3,4,4,5,4,4],
    ["Server City builder",4,3,4,4,3,3,4,4,4],
    ["Referral Boss",4,4,3,5,3,5,3,3,4],
    ["Mission board + достижения",5,3,3,4,4,5,5,4,4],
    ["Лиги маршрутов",3,4,4,4,3,3,3,3,3],
    ["Gift a Shield",5,5,3,5,4,5,5,5,4],
    ["Treasure Map без рандома",4,4,4,4,3,4,4,4,4],
    ["Коллекционные стикеры",5,4,3,3,5,4,5,5,4],
    ["Общая цель сообщества",5,3,3,2,5,5,5,5,4],
    ["Асинхронная экспедиция",3,4,4,4,3,3,3,4,4],
    ["Кейсы/рулетка с ценными призами",5,5,3,4,2,5,1,1,1],
], columns=["concept","clarity_5s","virality","retention","referral_link","reward_cost","mvp","antifraud","vpn_fit","reputation"])
games["total"] = games.iloc[:,1:].sum(axis=1)
games.sort_values("total", ascending=False)
"""
    ),
    md(
        """
## 8. Birthday discount break-even

При скидке 40% остаётся 60% цены. Без учёта затрат нужно увеличить число покупок в `1 / 0.6 = 1.667` раза. При цене месячного плана $3.13 и маржинальной себестоимости $0.05–0.24 необходимый uplift по contribution — примерно 69–76%. Массовая birthday-скидка без holdout почти наверняка каннибализирует обычные продления; поля даты рождения в базе нет.
"""
    ),
    code(
        """
P = 3.13
birthday = pd.DataFrame([
    {"marginal_cost":c, "required_purchase_multiplier":(P-c)/(0.6*P-c), "required_uplift_pct":100*((P-c)/(0.6*P-c)-1)}
    for c in [.05,.12,.24]
])
birthday
"""
    ),
    md(
        """
## 9. Минимальная система событий

Нужен единый `delivery_id` и `experiment_id` во всех сообщениях и событиях:

```sql
SELECT experiment_id, variant,
       COUNT(DISTINCT exposed_tg_id) AS exposed,
       COUNT(DISTINCT CASE WHEN event_name='referral_share_click' THEN tg_id END) AS share_clickers,
       COUNT(DISTINCT CASE WHEN event_name='referred_payment_success' THEN referrer_id END) AS paid_referrers,
       SUM(CASE WHEN event_name='referred_payment_success' THEN revenue_net_usd ELSE 0 END) AS net_revenue,
       SUM(reward_cost_usd + marginal_server_cost_usd + refund_usd) AS incremental_cost
FROM experiment_funnel
WHERE event_ts >= :start AND event_ts < :end
GROUP BY experiment_id, variant;
```

Обязательные события: `referral_screen_view`, `referral_copy`, `referral_share_click`, `referral_link_open`, `referred_signup`, `trial_started`, `payment_started`, `payment_success`, `hold_released`, `balance_redeemed`, `bot_blocked`, `notification_delivered`, `notification_clicked`.
"""
    ),
    md(
        """
## Вывод

Финальный выбор — не игра и не порог в три друга. Это пакет из двух проверяемых изменений: **20% сохраняются; заработанный баланс можно тратить на VPN с первого цента; друг после первой подтверждённой оплаты получает +7 дней**. Сначала тестируется ликвидность, затем recipient-benefit. Сезонный `Doodle Route` допускается только как третий эксперимент поверх уже доказанного оффера.
"""
    ),
    code(
        """
assert math.isclose(monthly.query("month == 'May'").revenue.iloc[0], 3581.06, abs_tol=.01)
assert math.isclose(monthly.query("month == 'July 1–30'").revenue.iloc[0], 2823.94, abs_tol=.01)
assert round(referrer_distribution.iloc[0].share_stopped_1_2, 3) == .780
assert active_payer_stock.active_payers.sum() == 1144
assert score.sort_values("owner", ascending=False).iloc[0].strategy == "Внутренний баланс + 7 дней другу"
assert birthday.required_uplift_pct.min() > 68

unit.to_csv(ROOT / "unit_economics.csv", index=False)
score.to_csv(ROOT / "strategy_scoring.csv", index=False)
games.to_csv(ROOT / "game_concepts.csv", index=False)
monthly.to_csv(ROOT / "monthly_metrics.csv", index=False)
print("All checks passed; analytical tables exported.")
"""
    ),
]


def execute_notebook(notebook: dict) -> None:
    namespace = {"math": math, "ROOT": ROOT}
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
                result = eval(compile(ast.Expression(tree.body[-1].value), f"cell-{execution_count}", "eval"), namespace)
            else:
                exec(compile(tree, f"cell-{execution_count}", "exec"), namespace)
        outputs = []
        printed = stdout.getvalue()
        if printed:
            outputs.append({"output_type": "stream", "name": "stdout", "text": printed})
        if result is not None:
            if hasattr(result, "to_html"):
                outputs.append({"output_type": "execute_result", "execution_count": execution_count, "metadata": {}, "data": {"text/plain": repr(result), "text/html": result.to_html()}})
            else:
                outputs.append({"output_type": "execute_result", "execution_count": execution_count, "metadata": {}, "data": {"text/plain": repr(result)}})
        cell["outputs"] = outputs


execute_notebook(nb)
OUT.write_text(json.dumps(nb, ensure_ascii=False, indent=1), encoding="utf-8")
print(OUT)
