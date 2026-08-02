import csv
import json
from pathlib import Path


ROOT = Path(__file__).parent


def read_csv(name):
    with (ROOT / name).open(encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    for row in rows:
        for key, value in list(row.items()):
            if value == "":
                row[key] = None
                continue
            try:
                row[key] = float(value)
            except (TypeError, ValueError):
                pass
    return rows


def sql_literal(value):
    if value is None:
        return "NULL"
    if isinstance(value, (int, float)):
        return str(value)
    return "'" + str(value).replace("'", "''") + "'"


def values_sql(name, rows, fields):
    values = ",\n  ".join("(" + ", ".join(sql_literal(row.get(field)) for field in fields) + ")" for row in rows)
    return f"WITH {name} ({', '.join(fields)}) AS (\n  VALUES {values}\n)\nSELECT * FROM {name};"


unit = read_csv("unit_economics.csv")
scores = read_csv("strategy_scoring.csv")

monthly = [
    {"month":"May", "segment":"New", "revenue":3006.33, "total":3581.06, "payers":644, "registrations":892, "aov":4.91},
    {"month":"May", "segment":"Returning", "revenue":574.73, "total":3581.06, "payers":644, "registrations":892, "aov":4.91},
    {"month":"June", "segment":"New", "revenue":1662.92, "total":3066.94, "payers":576, "registrations":627, "aov":4.78},
    {"month":"June", "segment":"Returning", "revenue":1404.02, "total":3066.94, "payers":576, "registrations":627, "aov":4.78},
    {"month":"July", "segment":"New", "revenue":1452.03, "total":2823.94, "payers":514, "registrations":420, "aov":4.98},
    {"month":"July", "segment":"Returning", "revenue":1371.91, "total":2823.94, "payers":514, "registrations":420, "aov":4.98},
]

drivers = [
    {"segment":"Special partners", "may":906.44, "july":486.52, "change":-419.92, "share":.555},
    {"segment":"Retail referrals 20%", "may":1203.32, "july":975.75, "change":-227.57, "share":.301},
    {"segment":"Non-ref / unattributed", "may":1471.30, "july":1361.67, "change":-109.63, "share":.145},
]

ref_activity = [
    {"month":"Apr 13–30", "registrations":142, "active_referrers":96, "regs_per_referrer":1.48},
    {"month":"May", "registrations":250, "active_referrers":163, "regs_per_referrer":1.53},
    {"month":"June", "registrations":208, "active_referrers":135, "regs_per_referrer":1.54},
    {"month":"July", "registrations":102, "active_referrers":83, "regs_per_referrer":1.23},
]

root_causes = [
    {"hypothesis":"Приток новых плательщиков упал", "support":"New revenue: $3,006 May → $1,452 July; registrations 892 → 420", "contradiction":"7-day conversion did not collapse", "missing":"Canonical acquisition cohort before Apr 13", "confidence":"High", "discriminator":"Weekly acquired paid users by attributable source"},
    {"hypothesis":"Просели специальные партнёры", "support":"−$420 May→July; top 10 explain −$428", "contradiction":"Other partners grew +$8", "missing":"Partner content cadence and reach", "confidence":"High", "discriminator":"Partner-level reactivation with matched historical baseline"},
    {"hypothesis":"Меньше обычных людей вообще реферят", "support":"163 active referrers May → 83 July; ~83% registration decline is count effect", "contradiction":"Referred users still convert ~35% in mature July cohort", "missing":"View/copy/share funnel", "confidence":"High", "discriminator":"Referral activation experiment with event funnel"},
    {"hypothesis":"Упал настоящий organic/direct", "support":"Non-ref registrations also fell", "contradiction":"Non-ref revenue only −$110; 73.5% source missing", "missing":"UTM/deep-link attribution", "confidence":"Medium-low", "discriminator":"Persist first/last touch for every start/payment"},
    {"hypothesis":"Notification fatigue harms conversion", "support":"Pretrial users got 18.8 repeated messages on average; only 5 paid within 7d", "contradiction":"No holdout, so no causal estimate", "missing":"Bot blocks, controls, delivery IDs", "confidence":"Medium", "discriminator":"Zero-message vs two-message randomized holdout"},
    {"hypothesis":"Payment failure caused decline", "support":"Crypto settled/attempt fell to ~31%", "contradiction":"Wata click→paid improved to 82.7%; crypto too small to explain total", "missing":"Provider-normalized intent funnel", "confidence":"Low", "discriminator":"Payment-intent cohort by provider and error"},
    {"hypothesis":"Retention collapsed", "support":"Possible definition-sensitive renewal softness", "contradiction":"Returning revenue $1,404 June vs $1,372 July", "missing":"Canonical eligible renewal event", "confidence":"Low/unknown", "discriminator":"Eligible renewal cohorts with plan-expiry logic"},
    {"hypothesis":"Price/AOV problem", "support":"None material", "contradiction":"AOV stable $4.91→$4.98", "missing":"Price elasticity test", "confidence":"Low", "discriminator":"Do not test until acquisition leak is fixed"},
    {"hypothesis":"Growing online proves healthy growth", "support":"Total MAU rose", "contradiction":"Core tg MAU fell 1,552→1,450; restore accounts rose 4→468", "missing":"Canonical customer-active metric", "confidence":"Rejected", "discriminator":"Exclude wl_restor* from business MAU"},
]

strategy_rows = []
base_unit = {r["strategy"]: r for r in unit if r["scenario"] == "base"}
score_map = {r["strategy"]: r for r in scores}
strategy_meta = {
    "Текущие 20%":("Деньги, вывод $10 USDT","Низкая ликвидность","Control"),
    "20%, вывод от $3":("Те же деньги, раньше вывод","Cash release existing liability","Test only if internal spend cannot ship"),
    "20% + мгновенная трата в VPN":("Баланс тратится с первого цента","Cash cannibalization/timing","Stage 1"),
    "7 дней рефереру вместо денег":("Фиксированные дни за оплату","May repel earners; contract change","Isolated test, not default"),
    "3 активных = бесплатный VPN":("Cliff at three active friends","78% stop at 1–2 registrations","Reject as first move"),
    "Выбор: деньги или VPN":("User chooses redemption","More UI complexity","Good fallback"),
    "20% + 7 дней другу после оплаты":("Real recipient benefit","Subsidizes baseline conversions","Stage 2"),
    "30-дневный referral-спринт":("Temporary boosted rewards","Fatigue and subsidy","Later campaign"),
    "Розыгрыш среди оплативших друзей":("Random prize","Legal, tax, fraud","Do not launch"),
    "Игра + фиксированный магазин":("Deterministic points store","Farmer traffic","Only after offer proof"),
    "Игра + случайные ценные призы":("Cases/roulette","Fraud, legal, reputation","Reject"),
    "Новые партнёры: $2 first-paid + 10% recurring":("Front-loaded partner bounty","Partner quality","New contracts only"),
    "Lifecycle/winback без скидки":("Behavior-triggered owned CRM","Needs measurement","Run in parallel"),
    "Внутренний баланс + 7 дней другу":("Liquid sender reward + friend benefit","Needs ≥24% lift in worst case","Recommended staged package"),
    "Гибрид + сезонный Doodle Route":("Same offer plus game layer","Game can hide weak offer","Stage 3 only"),
}
for label, (mechanic, risk, verdict) in strategy_meta.items():
    u = base_unit[label]
    s = score_map[label]
    strategy_rows.append({
        "strategy":label, "mechanic":mechanic, "base_incremental_payers":round(u["incremental_payers"],1),
        "base_net_contribution":round(u["net_incremental_contribution"],2), "score":round(s["owner"],1),
        "risk":risk, "verdict":verdict,
    })

finalists = ["20% + мгновенная трата в VPN", "20% + 7 дней другу после оплаты", "Внутренний баланс + 7 дней другу", "3 активных = бесплатный VPN", "Игра + фиксированный магазин", "Новые партнёры: $2 first-paid + 10% recurring"]
unit_finalists = []
for row in unit:
    if row["strategy"] in finalists:
        unit_finalists.append({
            "strategy":row["strategy"], "scenario":row["scenario"],
            "incremental_payers":round(row["incremental_payers"],1),
            "revenue":round(row["incremental_30d_revenue"],2),
            "current_obligation":round(row["existing_ref_obligation"],2),
            "server_cost":round(row["server_cost"] + row["free_access_infra"],2),
            "opportunity_cost":round(row["opportunity_cost"],2),
            "baseline_subsidy":round(row["baseline_reward_subsidy"],2),
            "external_prizes":round(row["external_prizes"],2),
            "fraud_reserve":round(row["fraud_reserve"],2),
            "net_contribution":round(row["net_incremental_contribution"],2),
            "break_even_payers":round(row["break_even_incremental_payers"],1),
        })

analogs = [
    {"analog":"Zefir VPN", "market_price":"RU Telegram VPN; 199 Stars/mo", "mechanic":"Friend gets 7d instead of 3; inviter +7d; 3 friends +30d; 5 subscribed friends = free while they renew", "threshold_hold":"1 / 3 / 5; paid-active condition", "applicability":"Very close product/market; performance and company size undisclosed", "source":"https://zefirvpn.ru/"},
    {"analog":"Volno", "market_price":"RU Telegram VPN; 199 ₽/mo", "mechanic":"Bonus days to both sides after friend’s first payment", "threshold_hold":"First payment; exact days not public", "applicability":"Close low-ticket benchmark; result data absent", "source":"https://volno.online/"},
    {"analog":"Delta VPN", "market_price":"RU VPN; 100 ₽/mo", "mechanic":"Friend 1 month, inviter 15 days", "threshold_hold":"Per friend; unlimited stated", "applicability":"Shows aggressive subsidy, not that it is profitable", "source":"https://deltavpn.ru/"},
    {"analog":"Tuneway", "market_price":"RU VPN; 249 ₽/mo", "mechanic":"Inviter +10d per subscribed friend; 9 friends = 3 months", "threshold_hold":"Subscribed friend", "applicability":"Simple one-sided days; higher price than DoodleVPN", "source":"https://dostup24.com/"},
    {"analog":"FastNeo VPN", "market_price":"RU Telegram VPN; 150 ₽/mo", "mechanic":"50% from every friend payment, withdrawal in bot", "threshold_hold":"Public withdrawal threshold not stated", "applicability":"Closest cash recurring analogue; economics/outcomes undisclosed", "source":"https://fastneoapp.com/"},
    {"analog":"Telegram Affiliate Programs", "market_price":"Telegram Mini Apps using Stars", "mechanic":"Developer sets revenue-share percent and duration; commissions in Stars", "threshold_hold":"Only referred Mini App Stars transactions", "applicability":"Potential infrastructure, not proof of user appeal; VPN advertising law still applies", "source":"https://core.telegram.org/api/bots/referrals"},
]

games = [
    {"concept":"Doodle Route seasonal map", "five_second":"5", "viral":"5", "d1_d7_d30":"4/4/3", "ref_dependency":"High", "reward_cost":"Low–medium", "mvp":"1–3d UI; 3–7d safe integration", "antifraud":"4/5", "vpn_fit":"4/5", "boredom":"Medium", "legal_rep":"Low if deterministic"},
    {"concept":"Internet Passport stamps", "five_second":"5", "viral":"4", "d1_d7_d30":"4/4/3", "ref_dependency":"Medium", "reward_cost":"Low", "mvp":"1–3d", "antifraud":"5/5", "vpn_fit":"4/5", "boredom":"Medium", "legal_rep":"Low"},
    {"concept":"Shield Team 3–5 people", "five_second":"4", "viral":"5", "d1_d7_d30":"4/4/3", "ref_dependency":"Very high", "reward_cost":"Medium", "mvp":"2–5d", "antifraud":"3/5", "vpn_fit":"4/5", "boredom":"Medium", "legal_rep":"Low–medium"},
    {"concept":"Gift a Shield", "five_second":"5", "viral":"5", "d1_d7_d30":"4/3/2", "ref_dependency":"Very high", "reward_cost":"Low", "mvp":"1–2d", "antifraud":"5/5", "vpn_fit":"5/5", "boredom":"High alone", "legal_rep":"Low"},
    {"concept":"Referral Relay chain", "five_second":"4", "viral":"5", "d1_d7_d30":"4/3/2", "ref_dependency":"Very high", "reward_cost":"Medium", "mvp":"2–4d", "antifraud":"3/5", "vpn_fit":"4/5", "boredom":"Medium", "legal_rep":"MLM optics risk"},
    {"concept":"Shield City co-op", "five_second":"4", "viral":"4", "d1_d7_d30":"4/4/3", "ref_dependency":"Medium", "reward_cost":"Low", "mvp":"3–7d", "antifraud":"4/5", "vpn_fit":"4/5", "boredom":"Medium", "legal_rep":"Low"},
    {"concept":"Privacy Garden / pet", "five_second":"5", "viral":"3", "d1_d7_d30":"4/4/3", "ref_dependency":"Low", "reward_cost":"Low", "mvp":"2–4d", "antifraud":"5/5", "vpn_fit":"4/5", "boredom":"Medium", "legal_rep":"Low"},
    {"concept":"Server City builder", "five_second":"4", "viral":"3", "d1_d7_d30":"4/4/3", "ref_dependency":"Medium", "reward_cost":"Low", "mvp":"3–7d", "antifraud":"4/5", "vpn_fit":"4/5", "boredom":"Medium", "legal_rep":"Low"},
    {"concept":"Referral Boss", "five_second":"4", "viral":"4", "d1_d7_d30":"4/3/2", "ref_dependency":"Very high", "reward_cost":"Medium", "mvp":"1–3d", "antifraud":"3/5", "vpn_fit":"3/5", "boredom":"High", "legal_rep":"Low"},
    {"concept":"Mission board + badges", "five_second":"5", "viral":"3", "d1_d7_d30":"4/3/2", "ref_dependency":"Medium", "reward_cost":"Low", "mvp":"1–3d", "antifraud":"5/5", "vpn_fit":"4/5", "boredom":"High", "legal_rep":"Low"},
    {"concept":"Route leagues", "five_second":"3", "viral":"4", "d1_d7_d30":"3/4/3", "ref_dependency":"High", "reward_cost":"Medium", "mvp":"3–7d", "antifraud":"3/5", "vpn_fit":"3/5", "boredom":"Medium", "legal_rep":"Competition risk"},
    {"concept":"Treasure Map deterministic", "five_second":"4", "viral":"4", "d1_d7_d30":"4/4/2", "ref_dependency":"High", "reward_cost":"Medium", "mvp":"2–4d", "antifraud":"4/5", "vpn_fit":"4/5", "boredom":"Medium", "legal_rep":"Low"},
    {"concept":"Collectible stickers", "five_second":"5", "viral":"4", "d1_d7_d30":"4/3/2", "ref_dependency":"Medium", "reward_cost":"Very low", "mvp":"1–3d", "antifraud":"5/5", "vpn_fit":"5/5", "boredom":"High", "legal_rep":"Low"},
    {"concept":"Community milestone", "five_second":"5", "viral":"3", "d1_d7_d30":"3/3/2", "ref_dependency":"Low", "reward_cost":"Low", "mvp":"1–3d", "antifraud":"5/5", "vpn_fit":"5/5", "boredom":"High", "legal_rep":"Low"},
    {"concept":"Async expedition", "five_second":"3", "viral":"4", "d1_d7_d30":"3/4/3", "ref_dependency":"High", "reward_cost":"Medium", "mvp":"3–7d", "antifraud":"3/5", "vpn_fit":"4/5", "boredom":"Medium", "legal_rep":"Low"},
    {"concept":"Cases / roulette with valuable prizes", "five_second":"5", "viral":"5", "d1_d7_d30":"4/3/1", "ref_dependency":"High", "reward_cost":"High", "mvp":"1–3d", "antifraud":"1/5", "vpn_fit":"1/5", "boredom":"High", "legal_rep":"High risk"},
]

lifecycle = [
    {"state":"New user", "timing":"Immediately", "goal":"Start trial", "suppression_cap":"Suppress if trial/payment; transactional; global cap", "control":"5%", "copy":"3 дня уже доступны. Подключить на этом устройстве →", "cta":"Подключить", "success":"trial_started"},
    {"state":"Trial not activated", "timing":"6h, then 36h only", "goal":"Complete setup", "suppression_cap":"Max 2 total; stop on trial/payment/support issue", "control":"10%", "copy":"VPN ещё не подключён. Покажем настройку за минуту.", "cta":"Настроить", "success":"first_connection"},
    {"state":"Trial active", "timing":"After first connection", "goal":"Confirm value", "suppression_cap":"Once", "control":"5%", "copy":"Готово — VPN работает. Если что-то не откроется, поможем здесь.", "cta":"Проверить", "success":"healthy_session"},
    {"state":"Trial ending", "timing":"24h before expiry", "goal":"First payment", "suppression_cap":"Once; suppress if paid", "control":"10%", "copy":"Пробный доступ закончится завтра. Месяц можно продлить в два нажатия.", "cta":"Выбрать срок", "success":"payment_success_48h"},
    {"state":"Viewed plans", "timing":"2h after exit", "goal":"Return to checkout", "suppression_cap":"Once/14d", "control":"10%", "copy":"Тариф сохранился. Вернуться к оплате?", "cta":"Продолжить", "success":"payment_started"},
    {"state":"Clicked payment", "timing":"30m", "goal":"Recover checkout", "suppression_cap":"Once per intent; suppress if success", "control":"10%", "copy":"Оплата не завершилась. Можно продолжить с того же места.", "cta":"Продолжить", "success":"payment_success_24h"},
    {"state":"Payment failed", "timing":"10m with provider error", "goal":"Offer working path", "suppression_cap":"Once/error", "control":"5%", "copy":"Платёж не прошёл. Попробовать другой способ?", "cta":"Другой способ", "success":"payment_success_24h"},
    {"state":"First payment", "timing":"Immediately", "goal":"Activation and trust", "suppression_cap":"Transactional", "control":"None", "copy":"Оплата прошла. Доступ активен до {date}.", "cta":"Подключить", "success":"first_paid_connection"},
    {"state":"Subscription expiry", "timing":"3d and 6h before", "goal":"Renewal", "suppression_cap":"Max 2; suppress if renewed", "control":"10%", "copy":"Доступ закончится через 3 дня. Продлить без перерыва?", "cta":"Продлить", "success":"renewal"},
    {"state":"Former payer", "timing":"7d after expiry, then 30d", "goal":"Winback", "suppression_cap":"Max 2/90d; no blanket discount", "control":"10%", "copy":"Нужен VPN снова? Подключение и прошлые устройства сохранены.", "cta":"Вернуться", "success":"reactivation"},
    {"state":"Referral page opened", "timing":"24h if no share", "goal":"First share", "suppression_cap":"Once/30d", "control":"10%", "copy":"Друг получит +7 дней после первой оплаты. Ссылка готова.", "cta":"Поделиться", "success":"referral_share_click"},
    {"state":"Link sent", "timing":"No reminder immediately", "goal":"Avoid pressure", "suppression_cap":"Status only", "control":"None", "copy":"Ссылка отправлена. Покажем статус, когда друг откроет бота.", "cta":"Статус", "success":"referral_link_open"},
    {"state":"First referred signup", "timing":"Immediately", "goal":"Explain next step", "suppression_cap":"Once/referral", "control":"5%", "copy":"Друг подключился. Награда появится после его первой оплаты.", "cta":"Статус", "success":"referred_payment"},
    {"state":"Referral paid", "timing":"After hold/release", "goal":"Make value liquid", "suppression_cap":"Transactional", "control":"None", "copy":"Друг оплатил: +${amount} на баланс. Им уже можно продлить VPN.", "cta":"Использовать", "success":"balance_redeemed"},
    {"state":"Near reward", "timing":"At deterministic threshold", "goal":"Finish progress", "suppression_cap":"Once/season; no fake urgency", "control":"10%", "copy":"До +3 дней осталась одна подтверждённая оплата друга.", "cta":"Пригласить", "success":"next_verified_referral"},
    {"state":"Reward received", "timing":"Immediately", "goal":"Close loop", "suppression_cap":"Transactional", "control":"None", "copy":"Готово: +3 дня добавлены до {date}.", "cta":"Открыть маршрут", "success":"reward_used"},
]

experiments = [
    {"experiment":"E0 Pretrial cleanup", "audience":"New non-payers", "treatment_control":"2 messages total vs 0", "primary":"7d payment rate", "guardrails":"Bot blocks, support tickets", "mde_sample":"Business MDE +25%; sequential, 6–8 weeks", "rollout":"Positive contribution and P(lift>10%)≥80%", "kill":"No lift after 8w or blocks +0.5pp", "contamination":"Hash by tg_id"},
    {"experiment":"E1 Liquid internal balance", "audience":"Active payers exposed to referral UI", "treatment_control":"Spend 20% balance in VPN from $0.01 vs $10 USDT only", "primary":"Verified paid referrals / 100 exposed", "guardrails":"Cash revenue displaced, withdrawal complaints", "mde_sample":"Target ≥25% lift; 50/50 for 6–8 weeks", "rollout":"≥60 verified payments, positive incremental contribution, posterior ≥80%", "kill":"P(lift≥10%)<20% after 8w", "contamination":"Referrer-level assignment"},
    {"experiment":"E2 Friend +7 after first payment", "audience":"New referral links", "treatment_control":"Real +7d recipient benefit vs standard trial", "primary":"Link-open → paid within 7d", "guardrails":"Next-renewal delay, refunds, fraud", "mde_sample":"Break-even ~16% worst-case; target ≥25%; 6–8 weeks", "rollout":"Incremental paid lift ≥25% and contribution >0", "kill":"Lift <10%, fraud >5%, or subsidy >$1/incremental $", "contamination":"Cluster by inviter/link variant"},
    {"experiment":"E3 Partner reactivation", "audience":"Top 10 declined partners", "treatment_control":"Personal reactivation kit; matched pre-period", "primary":"Verified first payers and net revenue", "guardrails":"Refunds, commission %, concentration", "mde_sample":"Small-N Bayesian/matched; 30 days", "rollout":"≥$2 contribution per acquired payer", "kill":"No verified payer from outreach cohort", "contamination":"Partner-specific links"},
    {"experiment":"E4 Doodle Route layer", "audience":"Only after E1+E2 win", "treatment_control":"Same offer, game UI vs plain UI", "primary":"Verified paid referrals / exposed", "guardrails":"Farmer rate, D7 opens without revenue, reward cost", "mde_sample":"Target ≥15% beyond offer; one 30-day season", "rollout":"Positive incremental profit and ≥15% lift", "kill":"No paid lift, farm >8%, or reward >10% referred revenue", "contamination":"Season assignment by referrer"},
]

channels = [
    {"channel":"Existing-base lifecycle / winback", "realism":"High", "cost":"Low", "speed":"Days", "legal":"Lower but review copy", "decision":"Do now with holdouts"},
    {"channel":"Gifting / family/group plan", "realism":"High", "cost":"Low–medium", "speed":"1–3 weeks", "legal":"Product mechanic; review promotion", "decision":"Prototype after attribution"},
    {"channel":"Reanimate top 10 partners", "realism":"High", "cost":"Performance-only", "speed":"Days", "legal":"VPN advertising gate", "decision":"Prepare; activate only after counsel"},
    {"channel":"New micro-partners", "realism":"Medium", "cost":"$2 first-paid + 10% recurring", "speed":"Weeks", "legal":"High advertising risk", "decision":"New contracts only; legal gate"},
    {"channel":"Platform/device onboarding", "realism":"High", "cost":"Low", "speed":"Days", "legal":"Support/product content", "decision":"Improve retention, not acquisition claim"},
    {"channel":"Cross-bot integrations", "realism":"Medium", "cost":"Rev-share", "speed":"Weeks", "legal":"Advertising risk", "decision":"Do not scale before counsel/attribution"},
    {"channel":"UGC, reviews, ambassadors", "realism":"Medium", "cost":"Low", "speed":"Weeks", "legal":"Advertising/endorsement risk", "decision":"No paid/public brief until counsel"},
    {"channel":"SEO/public VPN content", "realism":"Low–medium", "cost":"Time", "speed":"Months", "legal":"High in Russia", "decision":"Not a current recommendation"},
    {"channel":"Telegram Mini App discovery", "realism":"Low", "cost":"Low", "speed":"Uncertain", "legal":"Does not bypass VPN ad law", "decision":"Bonus distribution, not core plan"},
    {"channel":"B2B2C / digital bundles", "realism":"Medium", "cost":"Rev-share", "speed":"1–3 months", "legal":"Contract/regulatory review", "decision":"Explore after core recovery"},
]

plan = [
    {"horizon":"Tomorrow", "action":"Stop daily pretrial loop; keep transactional notices. Freeze threshold-3, roulette/cases and broad 40% discounts. Create legal review brief.", "owner_metric":"No user receives >2 pretrial prompts total"},
    {"horizon":"7 days", "action":"Implement referral/view/share/payment events, delivery_id, experiment_id and 10% persistent CRM holdout. Reconcile source attribution. Contact top 10 partners for diagnosis.", "owner_metric":"≥95% new starts and payments have first/last touch"},
    {"horizon":"30 days", "action":"Run E0 and E1. Ship internal balance redemption without changing earned 20% or old contracts. Build one referral page and one share message.", "owner_metric":"Referral funnel complete; cash displacement and contribution visible"},
    {"horizon":"60 days", "action":"If E1 passes, run E2 friend +7 after first confirmed payment. Pilot new-partner contract only after legal approval. Test gifting/family separately.", "owner_metric":"≥25% paid-referral lift and positive contribution"},
    {"horizon":"90 days", "action":"Only if E1+E2 pass, run one 30-day Doodle Route season against identical plain offer. Roll out winning CRM matrix.", "owner_metric":"Game adds ≥15% paid referrals beyond offer; reward ≤10% referred revenue"},
]

decision_rows = [
    {"priority":1, "decision":"Measurement + stop pretrial spam", "impact":"High", "confidence":"High", "effort":"Low", "now":"Yes"},
    {"priority":2, "decision":"Internal balance from first cent", "impact":"High", "confidence":"Medium-high", "effort":"Low", "now":"Experiment"},
    {"priority":3, "decision":"Friend +7 after first payment", "impact":"High", "confidence":"Medium", "effort":"Low", "now":"Stage 2 experiment"},
    {"priority":4, "decision":"Top-10 partner reactivation", "impact":"High", "confidence":"High diagnosis / legal gate", "effort":"Low", "now":"Prepare"},
    {"priority":5, "decision":"Lifecycle/winback without blanket discount", "impact":"Medium-high", "confidence":"Medium", "effort":"Low", "now":"Parallel tests"},
    {"priority":6, "decision":"New partner contract $2 +10%", "impact":"Medium", "confidence":"Medium", "effort":"Medium", "now":"New deals only"},
    {"priority":7, "decision":"Doodle Route season", "impact":"Potentially medium", "confidence":"Low", "effort":"Low UI / medium reliable ops", "now":"Only after core offer wins"},
    {"priority":8, "decision":"Threshold 3-active free VPN", "impact":"Low reach", "confidence":"Low", "effort":"Low", "now":"No"},
    {"priority":9, "decision":"Random prize/cases", "impact":"Uncertain", "confidence":"Low", "effort":"Low", "now":"No"},
]

sources = [
    {"id":"revenue_export", "label":"DoodleVPN validated revenue aggregate", "path":"analysis/doodlevpn-growth-research-2026-07-30/monthly_metrics.csv", "query":{"description":"Reviewed aggregate reproduced from accounting and bot mirrors; VALUES preserve the exact chart snapshot.", "engine":"SQLite", "sql":values_sql("monthly_snapshot", monthly, ["month","segment","revenue","total","payers","registrations","aov"]), "tables_used":["accounting.revenue","accounting.revenue_refunds","bot_mirror.users","bot_mirror.payments"], "filters":["paid_at >= 2026-04-13","paid_at < 2026-07-30 21:00:00 UTC","refunds matched to original source and external id"], "metric_definitions":["net revenue = amount_usd_net less matched refunds","new payer = first observed successful payment after 2026-04-13"]}},
    {"id":"driver_export", "label":"DoodleVPN revenue driver bridge", "query":{"description":"Reviewed May-to-July segment bridge; VALUES preserve the exact chart snapshot.", "engine":"SQLite", "sql":values_sql("driver_snapshot", drivers, ["segment","may","july","change","share"]), "tables_used":["accounting.revenue","accounting.revenue_refunds","bot_mirror.users"], "filters":["May and July 1–30 2026","payer segmented by current referrer commission classification"]}},
    {"id":"referral_export", "label":"DoodleVPN validated referral aggregate", "path":"analysis/doodlevpn-growth-research-2026-07-30/doodlevpn_growth_research.ipynb", "query":{"description":"Reviewed referral activity snapshot reproduced from bot mirror.", "engine":"SQLite", "sql":values_sql("referral_activity_snapshot", ref_activity, ["month","registrations","active_referrers","regs_per_referrer"]), "tables_used":["bot_mirror.users","bot_mirror.payments","bot_mirror.referral_holds","bot_mirror.withdrawal_requests"], "filters":["registration flow >= 2026-04-13","lifetime fields explicitly labeled","commission 20% = retail; other = special"]}},
    {"id":"unit_model", "label":"DoodleVPN scenario model", "path":"analysis/doodlevpn-growth-research-2026-07-30/unit_economics.csv", "query":{"description":"Transparent pessimistic/base/aggressive unit-economics sensitivity model; VALUES preserve the finalist snapshot.", "engine":"SQLite", "sql":values_sql("unit_finalists_snapshot", unit_finalists, ["strategy","scenario","incremental_payers","revenue","current_obligation","server_cost","opportunity_cost","baseline_subsidy","external_prizes","fraud_reserve","net_contribution","break_even_payers"]), "language":"Python", "metric_definitions":["incremental contribution subtracts current referral obligation, reward subsidy paid to baseline and incremental converters, server cost, opportunity cost, prizes, cannibalization and fraud reserve","scenario uplifts are assumptions, not forecasts"]}},
    {"id":"score_model", "label":"DoodleVPN 100-point strategy scoring", "path":"analysis/doodlevpn-growth-research-2026-07-30/strategy_scoring.csv", "query":{"description":"Owner-weighted and sensitivity scores; VALUES preserve the exact report snapshot.", "engine":"SQLite", "sql":values_sql("strategy_score_snapshot", scores, ["strategy","owner","profit_heavy","viral_heavy","risk_speed"]), "language":"Python"}},
    {"id":"root_analysis", "label":"Root-cause evidence matrix", "query":{"description":"Reviewed evidence and uncertainty matrix assembled from validated aggregates.", "engine":"SQLite", "sql":values_sql("root_cause_snapshot", root_causes, ["hypothesis","support","contradiction","missing","confidence","discriminator"])}},
    {"id":"strategy_analysis", "label":"Strategy comparison model", "query":{"description":"Reviewed mechanics, base economics and owner-weighted scores.", "engine":"SQLite", "sql":values_sql("strategy_snapshot", strategy_rows, ["strategy","mechanic","base_incremental_payers","base_net_contribution","score","risk","verdict"])}},
    {"id":"analog_analysis", "label":"Public analogue review", "query":{"description":"Public mechanics and applicability; no performance claims inferred.", "engine":"SQLite", "sql":values_sql("analogue_snapshot", analogs, ["analog","market_price","mechanic","threshold_hold","applicability","source"])}},
    {"id":"game_analysis", "label":"Game concept evaluation", "query":{"description":"Expert screen against requested product, retention, cost, fraud and reputation dimensions.", "engine":"SQLite", "sql":values_sql("game_snapshot", games, ["concept","five_second","viral","d1_d7_d30","ref_dependency","reward_cost","mvp","antifraud","vpn_fit","boredom","legal_rep"])}},
    {"id":"lifecycle_analysis", "label":"Lifecycle message design", "query":{"description":"State-triggered matrix with suppression, holdout and success events.", "engine":"SQLite", "sql":values_sql("lifecycle_snapshot", lifecycle, ["state","timing","goal","suppression_cap","control","copy","cta","success"])}},
    {"id":"experiment_analysis", "label":"Causal experiment plan", "query":{"description":"Sequential experiments with break-even and contamination rules.", "engine":"SQLite", "sql":values_sql("experiment_snapshot", experiments, ["experiment","audience","treatment_control","primary","guardrails","mde_sample","rollout","kill","contamination"])}},
    {"id":"channel_analysis", "label":"Zero-budget channel assessment", "query":{"description":"Channel realism, cost, speed and legal-risk screen.", "engine":"SQLite", "sql":values_sql("channel_snapshot", channels, ["channel","realism","cost","speed","legal","decision"])}},
    {"id":"plan_analysis", "label":"7/30/60/90 implementation plan", "query":{"description":"Ordered actions and exit metrics.", "engine":"SQLite", "sql":values_sql("plan_snapshot", plan, ["horizon","action","owner_metric"])}},
    {"id":"decision_analysis", "label":"Priority decision table", "query":{"description":"Priorities synthesized from evidence, economics and risk.", "engine":"SQLite", "sql":values_sql("decision_snapshot", decision_rows, ["priority","decision","impact","confidence","effort","now"])}},
    {"id":"law_281", "label":"Federal Law 281-FZ, official publication", "href":"https://publication.pravo.gov.ru/document/0001202507310012"},
    {"id":"prosecutor_vpn", "label":"Prosecutor's Office explanation of VPN advertising ban", "href":"https://epp.genproc.gov.ru/ru/proc_78/activity/legal-education/explain/otherwise/e8255163/"},
    {"id":"telegram_affiliate", "label":"Telegram Affiliate Programs", "href":"https://core.telegram.org/api/bots/referrals"},
    {"id":"telegram_miniapps", "label":"Telegram Mini Apps usage and Stars", "href":"https://telegram.org/blog/telegram-stars?setln=en"},
    {"id":"referral_cash", "label":"When Giving Money Does Not Work", "href":"https://www.sciencedirect.com/science/article/pii/S0167811613000906"},
    {"id":"prosocial", "label":"Why Prosocial Incentives Work", "href":"https://www.hbs.edu/ris/Publication%20Files/GershonCryderJohn%20-%20Why%20Prosocial%20Incentives%20Work_3a65737a-0749-4008-86f6-70aa9945db97.pdf"},
    {"id":"referral_value", "label":"Referral Programs and Customer Value", "href":"https://journals.sagepub.com/doi/abs/10.1509/jm.75.1.46"},
    {"id":"gamification_rct", "label":"2026 contest gamification randomized trial", "href":"https://link.springer.com/article/10.1007/s11129-026-09311-3"},
    {"id":"telegram_games", "label":"Telegram games retention report summary", "href":"https://www.theblock.co/amp/post/339563/telegram-games-had-trouble-earning-revenue-retaining-users-in-q4-report"},
    {"id":"tax_prizes", "label":"Federal Tax Service: prize taxation", "href":"https://www.nalog.gov.ru/rn77/taxation/taxes/ndfl/ndfl_fl/"},
]

charts = [
    {"id":"revenue_components", "title":"Monthly net revenue by payer status", "subtitle":"Returning revenue stayed near $1.4k while new-payer revenue halved from May.", "type":"bar", "dataset":"monthly", "sourceId":"revenue_export", "encodings":{"x":{"field":"month","type":"nominal","label":"Month"}, "y":{"field":"revenue","type":"quantitative","label":"Net revenue","format":"currency"}, "color":{"field":"segment","type":"nominal","label":"Payer status"}}, "valueFormat":"currency", "options":{"grouping":"stacked"}},
    {"id":"driver_bridge", "title":"May-to-July revenue change by acquisition segment", "subtitle":"Special partners account for 55.5% of the measured decline.", "type":"bar", "dataset":"drivers", "sourceId":"driver_export", "encodings":{"x":{"field":"segment","type":"nominal","label":"Segment"}, "y":{"field":"change","type":"quantitative","label":"Change","format":"currency"}}, "valueFormat":"currency"},
    {"id":"retail_activity", "title":"Retail referral activity", "subtitle":"Active referrers fell 163 → 83; registrations per active referrer also slipped.", "type":"line", "dataset":"ref_activity", "sourceId":"referral_export", "encodings":{"x":{"field":"month","type":"nominal","label":"Month"}, "y":{"fields":["registrations","active_referrers"],"type":"quantitative","label":"Count"}}},
    {"id":"strategy_scores", "title":"Strategy scores under the owner-specified weights", "subtitle":"The staged liquid-balance plus friend-benefit package is robust across weight sensitivities.", "type":"bar", "dataset":"scores", "sourceId":"score_model", "encodings":{"x":{"field":"strategy","type":"nominal","label":"Strategy"}, "y":{"field":"owner","type":"quantitative","label":"Score / 100"}}},
    {"id":"unit_scenarios", "title":"Incremental 30-day contribution of finalists", "subtitle":"Scenario uplift assumptions are not forecasts; all baseline reward subsidies are charged.", "type":"bar", "dataset":"unit_finalists", "sourceId":"unit_model", "encodings":{"x":{"field":"strategy","type":"nominal","label":"Strategy"}, "y":{"field":"net_contribution","type":"quantitative","label":"Incremental contribution","format":"currency"}, "color":{"field":"scenario","type":"nominal","label":"Scenario"}}, "valueFormat":"currency", "options":{"grouping":"grouped"}},
]

tables = [
    {"id":"root_causes", "title":"Root-cause hypotheses", "subtitle":"Evidence, counterevidence and the test that can distinguish each cause.", "dataset":"root_causes", "sourceId":"root_analysis", "defaultSort":{"field":"confidence_rank","direction":"asc"}, "columns":[{"field":"hypothesis","label":"Hypothesis"},{"field":"support","label":"Supporting evidence"},{"field":"contradiction","label":"Contradicting evidence"},{"field":"missing","label":"Missing data"},{"field":"confidence","label":"Confidence"},{"field":"discriminator","label":"Discriminating test"},{"field":"confidence_rank","label":"Rank","format":"number"}]},
    {"id":"strategies", "title":"Referral and growth strategy comparison", "subtitle":"Base-case economics are transparent assumptions, not causal forecasts.", "dataset":"strategy_rows", "sourceId":"strategy_analysis", "defaultSort":{"field":"score","direction":"desc"}, "columns":[{"field":"strategy","label":"Strategy"},{"field":"mechanic","label":"Mechanic"},{"field":"base_incremental_payers","label":"Base incremental payers","format":"number"},{"field":"base_net_contribution","label":"Base 30d contribution","format":"currency"},{"field":"score","label":"Score / 100","format":"number"},{"field":"risk","label":"Main risk"},{"field":"verdict","label":"Verdict"}]},
    {"id":"unit_finalists_table", "title":"Financial model of finalists", "subtitle":"Cash, infrastructure, opportunity cost and baseline subsidy remain separate.", "dataset":"unit_finalists", "sourceId":"unit_model", "defaultSort":{"field":"net_contribution","direction":"desc"}, "columns":[{"field":"strategy","label":"Strategy"},{"field":"scenario","label":"Scenario"},{"field":"incremental_payers","label":"Inc. payers","format":"number"},{"field":"revenue","label":"30d revenue","format":"currency"},{"field":"current_obligation","label":"20% obligation","format":"currency"},{"field":"server_cost","label":"Server + free access","format":"currency"},{"field":"opportunity_cost","label":"Opportunity cost","format":"currency"},{"field":"baseline_subsidy","label":"Baseline subsidy","format":"currency"},{"field":"external_prizes","label":"External prizes","format":"currency"},{"field":"fraud_reserve","label":"Fraud reserve","format":"currency"},{"field":"net_contribution","label":"Net contribution","format":"currency"},{"field":"break_even_payers","label":"Break-even inc. payers","format":"number"}]},
    {"id":"analogs", "title":"Closest public analogues", "subtitle":"Public mechanics are observable; company size, hold details and causal performance usually are not.", "dataset":"analogs", "sourceId":"analog_analysis", "defaultSort":{"field":"analog","direction":"asc"}, "columns":[{"field":"analog","label":"Analogue"},{"field":"market_price","label":"Market / price"},{"field":"mechanic","label":"Mechanic"},{"field":"threshold_hold","label":"Threshold / hold"},{"field":"applicability","label":"Applicability"},{"field":"source","label":"Source"}]},
    {"id":"games", "title":"Sixteen game concepts", "subtitle":"Scores are expert judgments; the game must beat an identical plain offer.", "dataset":"games", "sourceId":"game_analysis", "defaultSort":{"field":"concept","direction":"asc"}, "columns":[{"field":"concept","label":"Concept"},{"field":"five_second","label":"5-second clarity"},{"field":"viral","label":"Virality"},{"field":"d1_d7_d30","label":"D1/D7/D30"},{"field":"ref_dependency","label":"Referral dependence"},{"field":"reward_cost","label":"Reward cost"},{"field":"mvp","label":"MVP"},{"field":"antifraud","label":"Anti-fraud"},{"field":"vpn_fit","label":"VPN fit"},{"field":"boredom","label":"Boredom"},{"field":"legal_rep","label":"Legal/reputation"}]},
    {"id":"lifecycle", "title":"Lifecycle message matrix", "subtitle":"Global marketing cap: ≤2 messages in 7 days and ≤6 in 30 days; transactional notices excluded.", "dataset":"lifecycle", "sourceId":"lifecycle_analysis", "defaultSort":{"field":"order","direction":"asc"}, "columns":[{"field":"state","label":"State"},{"field":"timing","label":"Timing"},{"field":"goal","label":"Goal"},{"field":"suppression_cap","label":"Suppression / cap"},{"field":"control","label":"Holdout"},{"field":"copy","label":"Example copy"},{"field":"cta","label":"CTA"},{"field":"success","label":"Success event"},{"field":"order","label":"Order","format":"number"}]},
    {"id":"experiments", "title":"Causal experiment program", "subtitle":"One mechanism per test; small-N decisions use sequential Bayesian/Poisson evidence plus business break-even.", "dataset":"experiments", "sourceId":"experiment_analysis", "defaultSort":{"field":"order","direction":"asc"}, "columns":[{"field":"experiment","label":"Experiment"},{"field":"audience","label":"Audience"},{"field":"treatment_control","label":"Treatment / control"},{"field":"primary","label":"Primary metric"},{"field":"guardrails","label":"Guardrails"},{"field":"mde_sample","label":"MDE / sample / duration"},{"field":"rollout","label":"Rollout"},{"field":"kill","label":"Kill"},{"field":"contamination","label":"Avoid contamination"},{"field":"order","label":"Order","format":"number"}]},
    {"id":"channels", "title":"Zero-budget and performance-based channels", "subtitle":"Public VPN promotion in Russia is a legal gate, not a tactical afterthought.", "dataset":"channels", "sourceId":"channel_analysis", "defaultSort":{"field":"order","direction":"asc"}, "columns":[{"field":"channel","label":"Channel"},{"field":"realism","label":"Realism"},{"field":"cost","label":"Cost"},{"field":"speed","label":"Speed"},{"field":"legal","label":"Legal"},{"field":"decision","label":"Decision"},{"field":"order","label":"Order","format":"number"}]},
    {"id":"plan", "title":"7/30/60/90-day action plan", "subtitle":"Instrumentation and offer economics precede the game layer.", "dataset":"plan", "sourceId":"plan_analysis", "defaultSort":{"field":"order","direction":"asc"}, "columns":[{"field":"horizon","label":"Horizon"},{"field":"action","label":"Action"},{"field":"owner_metric","label":"Exit metric"},{"field":"order","label":"Order","format":"number"}]},
    {"id":"decisions", "title":"Decision table", "subtitle":"One recommended path, with explicit sequencing and stop conditions.", "dataset":"decisions", "sourceId":"decision_analysis", "defaultSort":{"field":"priority","direction":"asc"}, "columns":[{"field":"priority","label":"Priority","format":"number"},{"field":"decision","label":"Decision"},{"field":"impact","label":"Impact"},{"field":"confidence","label":"Confidence"},{"field":"effort","label":"Effort"},{"field":"now","label":"Now"}]},
]

for i, row in enumerate(root_causes, 1): row["confidence_rank"] = i
for i, row in enumerate(lifecycle, 1): row["order"] = i
for i, row in enumerate(experiments, 1): row["order"] = i
for i, row in enumerate(channels, 1): row["order"] = i
for i, row in enumerate(plan, 1): row["order"] = i

blocks = [
    {"id":"title","type":"markdown","body":"# DoodleVPN: как вернуть рост без рекламного бюджета"},
    {"id":"executive","type":"markdown","body":"## Executive Summary\n\n- **Падает не средний чек и не существующая платящая база, а приток новых плательщиков.** Май → июль: чистая выручка −21.1%, регистрации −52.9%, AOV $4.91 → $4.98; returning revenue июня и июля почти равна.\n- **Рефералка качественная, но перестала активироваться.** Обычный реферальный трафик конвертируется примерно вдвое лучше nonref, однако активных обычных рефереров стало 163 → 83. Одновременно десять специальных партнёров объясняют около $428 падения.\n- **Не заменять 20% порогом в три друга и не строить игру как лекарство.** Сохранить 20%, разрешить тратить заработанный баланс на VPN с первого цента, затем отдельно проверить реальную выгоду другу: +7 дней только после первой подтверждённой оплаты.\n- **Игра — третий эксперимент, не первый продукт.** Если два механизма выше дадут прибыльный lift, запустить один 30-дневный Doodle Route с детерминированными наградами; не делать cases, roulette, tap-to-earn и случайные денежные призы."},
    {"id":"what_changed","type":"markdown","body":"## Падение сосредоточено в новых плательщиках\n\nПоздняя оплата 30 июля добавила к исходному снимку $7.52, поэтому полный день закрывается на $2,823.94 и 514 плательщиках. Это не меняет вывод. Общий MAU нельзя использовать как доказательство роста: core `tg_*` MAU снизился 1,552 → 1,450, тогда как `wl_restor*` вырос 4 → 468."},
    {"id":"revenue_chart","type":"chart","chartId":"revenue_components"},
    {"id":"root_tree","type":"markdown","body":"## Root-cause tree: два доказанных драйвера и несколько недоказанных\n\n**Выручка = плательщики × частота × средний платёж.** Средний платёж стабилен; количество плательщиков падает. Внутри acquisition-потока одновременно просели специальные партнёры и массовая реферальная активность. Платежи, цена, retention, сезонность и спам остаются вторичными или недоказанными объяснениями."},
    {"id":"driver_chart","type":"chart","chartId":"driver_bridge"},
    {"id":"root_table","type":"table","tableId":"root_causes"},
    {"id":"ref_verdict","type":"markdown","body":"## Вердикт по текущим 20%: оставить экономику, исправить ликвидность и social value\n\n20% не доказаны как плохой размер: 30-дневная выручка обычного реферального плательщика $6.98 против $6.03 nonref, а июльская 7-дневная конверсия около 35% против 17.4%. Проблема — 16 оплат по ~$0.63 до вывода $10, USDT-only и отсутствие выгоды другу сверх стандартного trial.\n\nСтрого после 13 апреля 78.0% рефереров остановились на 1–2 регистрациях; среди 1,144 активных плательщиков 960 не имеют ни одного активного платного друга. Поэтому cliff «3 активных = free VPN» имеет слишком малый достижимый reach."},
    {"id":"ref_chart","type":"chart","chartId":"retail_activity"},
    {"id":"strategy_chart","type":"chart","chartId":"strategy_scores"},
    {"id":"strategy_table","type":"table","tableId":"strategies"},
    {"id":"econ","type":"markdown","body":"## Финансовая модель: baseline subsidy посчитана полностью\n\nМодель списывает награду и на дополнительных, и на тех плательщиков, которые пришли бы без изменения. Поэтому рекомендуемый пакет убыточен в pessimistic (−$9.07/месяц при +18% lift), но положителен в base (+$36.64 при +40%). Worst-case break-even — около 8.2 дополнительных плательщика, то есть примерно +24% к июльской базе 35. Это и есть rollout-порог, а не обещание результата."},
    {"id":"unit_chart","type":"chart","chartId":"unit_scenarios"},
    {"id":"unit_table","type":"table","tableId":"unit_finalists_table"},
    {"id":"external","type":"markdown","body":"## Внешние аналоги не дают готового ответа\n\nРоссийские low-ticket VPN используют одновременно days, thresholds, internal value и recurring cash. Это показывает допустимость механик, но не их causal performance. Исследования также неоднозначны: денежные награды могут увеличивать social cost для слабого бренда, а recipient-benefiting incentives иногда генерируют referrals не хуже sender rewards. Поэтому DoodleVPN должен тестировать выгоду другу отдельно от ликвидности отправителя. [Cash vs in-kind study](https://www.sciencedirect.com/science/article/pii/S0167811613000906), [prosocial incentive research](https://www.hbs.edu/ris/Publication%20Files/GershonCryderJohn%20-%20Why%20Prosocial%20Incentives%20Work_3a65737a-0749-4008-86f6-70aa9945db97.pdf)."},
    {"id":"analog_table","type":"table","tableId":"analogs"},
    {"id":"legal","type":"markdown","body":"## Юридический gate меняет весь zero-budget план\n\nС 1 сентября 2025 года в России действует ответственность за рекламу VPN и иных средств обхода ограничений. [Официальный текст 281-ФЗ](https://publication.pravo.gov.ru/document/0001202507310012); [разъяснение прокуратуры](https://epp.genproc.gov.ru/ru/proc_78/activity/legal-education/explain/otherwise/e8255163/). Это не юридическое заключение, но достаточно, чтобы не запускать публичные интеграции, инфлюенсеров, SEO-продвижение, UGC-брифы или Telegram Affiliate как будто они автоматически безопасны. До rollout нужен письменный review конкретного оффера, share-текста и канала российским юристом."},
    {"id":"game_verdict","type":"markdown","body":"## Вердикт по игре: строить только как отдельный измеримый слой\n\nTelegram даёт нативную среду Mini Apps и affiliate infrastructure, но clicker boom не является proof of monetization. Отраслевой отчёт по Telegram games приводил retention порядка 5–20%; академические результаты показывают, что временные contests способны поднять engagement, но эффект зависит от контекста и может исчезнуть после наград. [Telegram platform](https://telegram.org/blog/telegram-stars?setln=en), [2026 RCT](https://link.springer.com/article/10.1007/s11129-026-09311-3).\n\nТоп-3 полноценные концепции: **Doodle Route**, **Internet Passport**, **Shield Team**. `Gift a Shield` набирает больше как referral-механика, но сам по себе не удерживает D30; его нужно встроить в Doodle Route."},
    {"id":"games_table","type":"table","tableId":"games"},
    {"id":"winner","type":"markdown","body":"## Победитель: 30-дневный Doodle Route\n\n**Core loop:** открыть карту → увидеть следующий checkpoint → выполнить продуктовую/социальную миссию → подтверждённая оплата друга двигает маршрут → получить фиксированную награду → подарить другу +7 дней. Нет paid points, multi-level и оплаты попыток.\n\n**Первый сеанс:** 10 секунд: карта из 6 checkpoint, готовая ссылка и текст «Другу +7 дней после первой оплаты; тебе 20%, которые можно сразу потратить на VPN». После share показывается только статус, без ежедневного давления.\n\n**Points:** daily puzzle 5, сезонный cap 100 (только cosmetics); first verified referred payment 600; referral renewal 120; own renewal 60. Sinks: 300 cosmetic stamp; 600 +3 VPN days; 1,200 extra device for 30d; 1,800 +7 days; 3,600 +14 days. Дополнительная стоимость наград ≤10% net referred revenue.\n\n**Anti-fraud:** no value for signup/trial; paid + hold + refund check; immutable inviter; device/payment/risk graph; household IP is a signal, not a ban; 1,200 valuable points/month before manual review.\n\n**Notifications:** season start once, first friend paid transactional, near deterministic checkpoint once, season end once. **MVP:** one map, 6 checkpoints, fixed store, four events; no leaderboard/clans/random rewards. **Kill:** no ≥15% paid-referral lift over identical offer after 30 days, farmer share >8%, or reward cost >10% referred net revenue."},
    {"id":"birthday","type":"markdown","body":"## Birthday, anniversary, winback and family\n\nA blanket birthday discount of 40% needs about **69–76% more purchases** to preserve contribution at a $3.13 monthly plan and $0.05–$0.24 marginal cost. No birthday field exists in the database, so collecting unreliable dates to trigger a destructive discount is not justified.\n\nRecommended order: anniversary-of-first-payment gift (3 days, existing data) → winback without discount → targeted 15–20% offer only to users expired ≥30 days in a holdout test → gifting/family plan. A free birthday badge or 1–3 days can be tested only after a real date-collection consent flow exists."},
    {"id":"lifecycle_md","type":"markdown","body":"## Новая lifecycle-система replaces broadcast pressure\n\nDisable the daily pretrial loop. Marketing cap: no more than 2 messages in 7 days and 6 in 30 days; transactional payment, expiry and reward confirmations are excluded. Every send carries `delivery_id`, `experiment_id`, `variant`, suppression reason and a persistent 5–10% holdout."},
    {"id":"lifecycle_table","type":"table","tableId":"lifecycle"},
    {"id":"partners","type":"markdown","body":"## Специальные партнёры — отдельный продукт\n\nTen partners fell from $801.31 in May to $373.21 in July (−$428.10); all others together grew about $8. July special commissions were $207.95 on $413.48 base, roughly 50.3%.\n\nDo not rewrite old deals retroactively. Reactivate the top ten with partner-specific diagnosis and current terms. For **new** contracts test `$2 after verified first payment + 10% recurring`, 30-day hold, refunds netted, tiers only after 10/20 verified new payers, no registration bounty. At $5.74 30-day revenue this costs about $2.57 before infra and leaves about $2.93 contribution at $0.24 infra. Offer existing partners optional migration only with a guaranteed floor."},
    {"id":"attribution","type":"markdown","body":"## Attribution and anti-fraud must exist before incentives scale\n\nPersist immutable `first_touch`, mutable `last_touch`, `referrer_id`, `campaign_id`, `partner_id`, deep-link token and experiment variant on start; copy them into every payment and refund. Required funnel events: referral view, copy, share click, link open, signup, trial, payment intent, success, hold release, balance redeem, notification delivery/click and bot block.\n\nReward only confirmed revenue after hold; reverse on refund; one inviter per payer; no reward for registration/trial; cap velocity and review shared devices/payment instruments. Do not hard-block shared IPs because households and VPN exits collide. Keep referral and partner ledgers separate."},
    {"id":"experiments_md","type":"markdown","body":"## Experiments: causal answers with a small audience\n\nAt a 6.6% monthly referrer activation rate, a classical A/B is underpowered for modest lifts. Use 50/50 referrer-level assignment, sequential Bayesian/Poisson monitoring and business break-even. Do not test liquidity, friend gift and game in one treatment: E1, E2 and E4 are sequential by design."},
    {"id":"experiments_table","type":"table","tableId":"experiments"},
    {"id":"channels_md","type":"markdown","body":"## Other zero-budget channels\n\nThe highest-realism channels are lifecycle/winback, gifting/family mechanics and reactivation of already-known partners. Public acquisition tactics remain gated by law and attribution. Telegram Affiliate Programs can technically pay Stars commissions for Mini App purchases, but do not bypass Russian VPN-advertising restrictions and do not solve the consumer-offer problem."},
    {"id":"channels_table","type":"table","tableId":"channels"},
    {"id":"plan_md","type":"markdown","body":"## 7/30/60/90-day plan\n\nThe sequence deliberately separates measurement, sender liquidity, recipient value and game wrapper. Each later stage requires the previous stage to produce incremental contribution."},
    {"id":"plan_table","type":"table","tableId":"plan"},
    {"id":"dont","type":"markdown","body":"## What not to do\n\n- Do not call growing total MAU growth; exclude restore accounts from business MAU.\n- Do not replace 20% with the three-active cliff now.\n- Do not lower payout and confuse release of existing liability with new acquisition cost.\n- Do not launch a game, cases, roulette, random cash/ChatGPT prizes or tap-to-earn before offer lift.\n- Do not give value for signup, trial, clicks or daily opens.\n- Do not run a 40% birthday discount to all renewers.\n- Do not renegotiate special partners retroactively.\n- Do not send 19 pretrial messages or evaluate CRM without a holdout.\n- Do not label unattributed traffic organic.\n- Do not launch influencer, SEO, UGC or cross-bot VPN promotion without a specific legal review."},
    {"id":"decision_md","type":"markdown","body":"## Final decision\n\nThe one recommended direction is a **staged two-sided liquid referral offer**: keep 20%; allow spending the earned balance on DoodleVPN from the first cent; after that proves activation, give the friend +7 days only after a verified first payment. The public copy, subject to legal approval: **«Другу — +7 дней после первой оплаты. Тебе — 20% каждой его оплаты. Баланс можно сразу потратить на VPN или вывести от $10.»**"},
    {"id":"decision_table","type":"table","tableId":"decisions"},
    {"id":"fallback","type":"markdown","body":"## Alternative and pre-mortem\n\n**Fallback if the main package fails:** keep 20%, add internal redemption only, focus on lifecycle/winback and new-partner `$2 + 10%` contracts; do not add a game.\n\n**Pre-mortem:** (1) people still feel awkward advertising VPN to friends; (2) +7 days subsidizes users who would already pay; (3) internal balance cannibalizes cash renewals; (4) legal review blocks public share wording; (5) partner concentration returns; (6) restore/attribution bugs make the experiment look successful; (7) a game produces opens and farmers, not payments. Each is covered by a holdout, contribution metric, legal gate, attribution completeness, partner concentration limit or explicit kill criterion."},
    {"id":"tomorrow","type":"markdown","body":"## Exact owner sequence\n\n1. **Tomorrow disable:** daily pretrial repeats; random-prize and threshold-3 implementation; blanket birthday discount.\n2. **Measure:** the complete referral/share/payment funnel, bot blocks, attribution completeness and incremental contribution.\n3. **Change in bot:** internal balance redemption from $0.01; existing $10 USDT withdrawal remains.\n4. **Show first:** E1 to 50% of eligible active payers; do not add friend bonus yet.\n5. **After E1 wins:** E2 offer +7 days to the friend after first confirmed payment. Budget at full July baseline: about $14/month base opportunity+infra, about $28 pessimistic; hard cap $1 total incremental reward per verified payer.\n6. **Game:** one 30-day Doodle Route, only after E1+E2; fixed product rewards; $0 external-prize budget in MVP, then at most $50/month and ≤10% of referred net revenue.\n7. **Look at results:** weekly guardrails; decision after 6–8 weeks and at least 60 verified referred payments across E1/E2.\n8. **Scale:** paid referrals +25% or more, posterior probability of meaningful lift ≥80%, incremental contribution >0, fraud <5%, bot-block delta <0.5 pp.\n9. **Close:** lift <10%, negative contribution, fraud >5%, or game farm >8% / no ≥15% incremental paid lift."},
    {"id":"caveats","type":"markdown","body":"## Caveats and assumptions\n\nThe July full-day snapshot differs from the owner’s earlier snapshot by one late payment. `New` may include people whose pre-April payment is outside the observable window. Nonref attribution is incomplete. Renewal is not canonical. Marginal server cost is assumed at $0.05/$0.12/$0.24 because $308 July cash expense and $792 listed monthly server inventory do not reconcile into capacity cost. Strategy uplifts and scores are priors for decision analysis, not predictions. External analogues publish mechanics, not outcomes. Legal notes are risk flags, not legal advice."},
]

artifact = {
    "surface":"report",
    "manifest":{
        "version":1, "surface":"report", "title":"DoodleVPN: как вернуть рост без рекламного бюджета",
        "description":"Independent growth, referral, lifecycle, game and unit-economics decision report.",
        "generatedAt":"2026-07-30T21:00:00Z", "sources":sources, "charts":charts, "tables":tables, "blocks":blocks,
    },
    "snapshot":{
        "version":1, "generatedAt":"2026-07-30T21:00:00Z", "status":"ready",
        "datasets":{
            "monthly":monthly, "drivers":drivers, "ref_activity":ref_activity, "root_causes":root_causes,
            "strategy_rows":strategy_rows, "unit_finalists":unit_finalists, "scores":scores, "analogs":analogs,
            "games":games, "lifecycle":lifecycle, "experiments":experiments, "channels":channels,
            "plan":plan, "decisions":decision_rows,
        },
    },
    "sources":sources,
}

(ROOT / "artifact.json").write_text(json.dumps(artifact, ensure_ascii=False, indent=2), encoding="utf-8")
print(ROOT / "artifact.json")
