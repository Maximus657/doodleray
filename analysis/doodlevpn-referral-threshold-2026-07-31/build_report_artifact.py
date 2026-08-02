from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent

gift_rows = [
    {"label": "Май: до", "event": "+3 дня", "phase": "До", "phase_order": 1, "payment_type": "Первая", "count": 16, "total": 41, "payers": 41, "days": 3, "recipients": 5179},
    {"label": "Май: до", "event": "+3 дня", "phase": "До", "phase_order": 1, "payment_type": "Повторная", "count": 25, "total": 41, "payers": 41, "days": 3, "recipients": 5179},
    {"label": "Май: подарок", "event": "+3 дня", "phase": "Подарок", "phase_order": 2, "payment_type": "Первая", "count": 13, "total": 27, "payers": 24, "days": 3, "recipients": 5179},
    {"label": "Май: подарок", "event": "+3 дня", "phase": "Подарок", "phase_order": 2, "payment_type": "Повторная", "count": 14, "total": 27, "payers": 24, "days": 3, "recipients": 5179},
    {"label": "Май: после", "event": "+3 дня", "phase": "После", "phase_order": 3, "payment_type": "Первая", "count": 32, "total": 48, "payers": 47, "days": 3, "recipients": 5179},
    {"label": "Май: после", "event": "+3 дня", "phase": "После", "phase_order": 3, "payment_type": "Повторная", "count": 16, "total": 48, "payers": 47, "days": 3, "recipients": 5179},
    {"label": "Июль: до", "event": "+7 дней", "phase": "До", "phase_order": 1, "payment_type": "Первая", "count": 29, "total": 147, "payers": 141, "days": 7, "recipients": 4707},
    {"label": "Июль: до", "event": "+7 дней", "phase": "До", "phase_order": 1, "payment_type": "Повторная", "count": 118, "total": 147, "payers": 141, "days": 7, "recipients": 4707},
    {"label": "Июль: подарок", "event": "+7 дней", "phase": "Подарок", "phase_order": 2, "payment_type": "Первая", "count": 19, "total": 56, "payers": 54, "days": 7, "recipients": 4707},
    {"label": "Июль: подарок", "event": "+7 дней", "phase": "Подарок", "phase_order": 2, "payment_type": "Повторная", "count": 37, "total": 56, "payers": 54, "days": 7, "recipients": 4707},
    {"label": "Июль: после", "event": "+7 дней", "phase": "После", "phase_order": 3, "payment_type": "Первая", "count": 47, "total": 150, "payers": 146, "days": 7, "recipients": 4707},
    {"label": "Июль: после", "event": "+7 дней", "phase": "После", "phase_order": 3, "payment_type": "Повторная", "count": 103, "total": 150, "payers": 146, "days": 7, "recipients": 4707},
]

threshold_rows = [
    {"threshold": "1 друг", "july_qualifiers": 51, "service_months": 51.0, "share_of_referrers": 1.0, "high_cost": 171.87, "verdict": "Слишком большой baseline subsidy"},
    {"threshold": "2 друга", "july_qualifiers": 6, "service_months": 6.0, "share_of_referrers": 0.118, "high_cost": 20.22, "verdict": "Лучший баланс силы и риска"},
    {"threshold": "3 друга", "july_qualifiers": 0, "service_months": 0.0, "share_of_referrers": 0.0, "high_cost": 0.0, "verdict": "Недостижим в июльском потоке"},
]

variant_rows = [
    {"priority": 1, "model": "30 дней за 2 новые первые оплаты", "headline": "Сильный", "july_service_months": 6.0, "added_cost_low": 9.36, "added_cost_high": 20.22, "cannibalization": "Только у реферера; ограничено", "fraud": "Средний, контролируемый", "decision": "Тестировать 30 дней"},
    {"priority": 2, "model": "14 дней за каждую первую оплату", "headline": "Средний", "july_service_months": 26.6, "added_cost_low": 41.50, "added_cost_high": 89.64, "cannibalization": "Широкая субсидия", "fraud": "Средний", "decision": "Резерв, если порог 2 не активирует"},
    {"priority": 3, "model": "Текущие 20% + понятный UX", "headline": "Слабый без события", "july_service_months": 0.0, "added_cost_low": 0.0, "added_cost_high": 0.0, "cannibalization": "Баланс может замещать оплату", "fraud": "Низкий", "decision": "Оставить фундаментом в обеих группах"},
    {"priority": 4, "model": "7 дней за каждую первую оплату", "headline": "Слабее", "july_service_months": 13.3, "added_cost_low": 20.75, "added_cost_high": 44.82, "cannibalization": "Широкая субсидия", "fraud": "Средний", "decision": "Проигрывает 2→30"},
    {"priority": 5, "model": "+7 дней приглашённому после оплаты", "headline": "Средний", "july_service_months": 13.3, "added_cost_low": 20.75, "added_cost_high": 44.82, "cannibalization": "Напрямую отодвигает первое продление", "fraud": "Средний", "decision": "Не запускать"},
    {"priority": 6, "model": "30 дней за 1 первую оплату", "headline": "Очень сильный", "july_service_months": 51.0, "added_cost_low": 79.56, "added_cost_high": 171.87, "cannibalization": "51 бесплатный месяц до lift", "fraud": "Высокий", "decision": "Не запускать"},
    {"priority": 7, "model": "30 дней за 3 первые оплаты", "headline": "Слабый из-за cliff", "july_service_months": 0.0, "added_cost_low": 0.0, "added_cost_high": 0.0, "cannibalization": "Низкая", "fraud": "Низкий", "decision": "Не запускать первым"},
    {"priority": 8, "model": "Лотерея / кейсы / игра", "headline": "Неизвестный", "july_service_months": 0.0, "added_cost_low": 0.0, "added_cost_high": 0.0, "cannibalization": "Неизвестная", "fraud": "Высокий", "decision": "Только после выигрыша оффера"},
]

plan_rows = [
    {"order": 1, "when": "Завтра", "action": "Зафиксировать правила: только ordinary; только новые direct first-paid VPN plans; одна награда; без ретроактива; 48h hold; друг получает обычный 3-дневный trial.", "owner_metric": "Письменная спецификация без двусмысленности"},
    {"order": 2, "when": "Дни 1–3", "action": "Одинаково упростить referral UX в control и treatment; явно написать, что баланс тратится от $1, а $10 нужен только для USDT-вывода. Добавить view/share/link-open/first-paid/reward events.", "owner_metric": "≥95% referral и payment событий имеют variant/referrer"},
    {"order": 3, "when": "День 4", "action": "Случайно закрепить ordinary active payers: 70% treatment, 30% текущий 20% control. Treatment видит оффер 2→30; одна стартовая рассылка, без ежедневных напоминаний.", "owner_metric": "Группы закреплены по referrer_id; special исключены"},
    {"order": 4, "when": "Дни 4–30", "action": "Показывать прогресс 0/2, 1/2, 2/2. Единственное нетранзакционное напоминание — пользователям 1/2 через 7 дней.", "owner_metric": "Ни одной награды за signup/trial; refund reverses eligibility"},
    {"order": 5, "when": "День 30", "action": "Считать incremental first-paid = paid_T − eligible_T/eligible_C × paid_C. Полностью списать 20%, все выданные месяцы по $3.37 и fraud/refunds.", "owner_metric": "Scale: ≥15 incremental paid, ≥35% rate lift, contribution >0"},
    {"order": 6, "when": "После решения", "action": "Если проходит — второй сезон с тем же оффером; только тогда можно обернуть прогресс в простую игру. Если нет — убрать месяц и тестировать 14 дней за первую оплату.", "owner_metric": "Kill: incremental ≤0, contribution ≤0, fraud/refund >5%, blocks +0.5 pp"},
]

sources = [
    {
        "id": "giveaway_db",
        "label": "DoodleVPN production payments and mass grants",
        "query": {
            "description": "Equal-length windows before, during and after the May and July free-day grants.",
            "engine": "SQLite",
            "tables_used": ["platega_payments", "crypto_payments", "payments", "mass_grant_runs", "mass_grant_results", "broadcasts", "broadcast_jobs", "user_events"],
            "filters": ["successful payments only", "May 19–28 and July 9–30, 2026", "provider success timestamp", "no personal identifiers exported"],
            "metric_definitions": ["first payment = earliest successful payment for tg_id", "repeat payment = later successful payment", "July recipient count = successfully sent compensation broadcast jobs"],
            "sql": "WITH success AS (SELECT paid_at ts,tg_id FROM platega_payments WHERE status='paid' AND paid_at IS NOT NULL UNION ALL SELECT paid_at,tg_id FROM crypto_payments WHERE status='paid' AND paid_at IS NOT NULL UNION ALL SELECT created_at,tg_id FROM payments WHERE status='paid' AND method='stars'), firsts AS (SELECT tg_id,MIN(ts) first_ts FROM success GROUP BY tg_id) SELECT date(ts),COUNT(*),SUM(ts=first_ts),SUM(ts>first_ts) FROM success JOIN firsts USING(tg_id) WHERE ts>='2026-05-19' AND ts<'2026-07-31' GROUP BY date(ts);",
        },
    },
    {
        "id": "threshold_db",
        "label": "DoodleVPN ordinary referral paid-friend cohorts",
        "query": {
            "description": "Monthly count of ordinary referrers reaching one, two or three new first-paid direct friends.",
            "engine": "SQLite",
            "tables_used": ["users", "payments"],
            "filters": ["first paid at or after 2026-04-13", "referrer commission = 20%", "successful payments only"],
            "metric_definitions": ["qualifier = referrer with at least N newly first-paid direct friends inside calendar month", "July ordinary first-paid friends = 57"],
            "sql": "WITH first_paid AS (SELECT tg_id,MIN(created_at) first_paid_at FROM payments WHERE status IN('paid','completed') GROUP BY tg_id), monthly AS (SELECT strftime('%Y-%m',fp.first_paid_at) month,child.referrer_id,COUNT(*) paid_friends FROM users child JOIN first_paid fp ON fp.tg_id=child.tg_id JOIN users r ON r.tg_id=child.referrer_id WHERE fp.first_paid_at>='2026-04-13' AND COALESCE(r.ref_commission_pct,20)=20 GROUP BY month,child.referrer_id) SELECT month,SUM(paid_friends),COUNT(*),SUM(paid_friends>=2),SUM(paid_friends>=3) FROM monthly GROUP BY month;",
        },
    },
    {"id": "ux_audit", "label": "DoodleVPN referral UX production audit", "path": "analysis/doodlevpn-bot-ux-audit-2026-07-31/report.md"},
    {"id": "analysis_model", "label": "DoodleVPN referral threshold notebook", "path": "analysis/doodlevpn-referral-threshold-2026-07-31/doodlevpn_referral_threshold.ipynb"},
    {"id": "variant_model", "label": "DoodleVPN referral option cost model", "query": {"description": "Reviewed option snapshot using July qualifying counts and a $1.56–$3.37 service-month cost range.", "engine": "SQLite", "tables_used": ["users", "payments"], "filters": ["ordinary 20% referrers", "July 2026 first-paid direct friends"], "metric_definitions": ["low cost = service months × ($1.32 internal value + $0.24 infrastructure)", "high cost = service months × ($3.13 monthly net + $0.24 infrastructure)"], "sql": "WITH option_snapshot(priority,model,july_service_months,added_cost_low,added_cost_high) AS (VALUES (1,'30 days after 2',6.0,9.36,20.22),(2,'14 days after each',26.6,41.50,89.64),(3,'current 20 percent',0,0,0),(4,'7 days after each',13.3,20.75,44.82),(5,'friend plus 7',13.3,20.75,44.82),(6,'30 days after 1',51.0,79.56,171.87),(7,'30 days after 3',0,0,0),(8,'lottery or game',0,0,0)) SELECT * FROM option_snapshot;"}},
    {"id": "plan_model", "label": "DoodleVPN controlled rollout plan", "query": {"description": "Ordered implementation and decision checklist derived from the validated threshold model.", "engine": "SQLite", "sql": "WITH rollout_step(step_no,timing) AS (VALUES (1,'tomorrow'),(2,'days 1-3'),(3,'day 4'),(4,'days 4-30'),(5,'day 30'),(6,'after decision')) SELECT * FROM rollout_step ORDER BY step_no;"}},
    {"id": "threshold_research", "label": "Social Referral Programs for Freemium Platforms", "href": "https://pubsonline.informs.org/doi/10.1287/mnsc.2022.4301"},
]

charts = [
    {"id": "gift_windows", "title": "Успешные оплаты вокруг массовых подарков", "subtitle": "Равные окна до, во время и после начисления; июльское post-окно включает 44 покупки со скидкой 37%.", "type": "bar", "dataset": "gift_rows", "sourceId": "giveaway_db", "encodings": {"x": {"field": "label", "type": "nominal", "label": "Окно"}, "y": {"field": "count", "type": "quantitative", "label": "Оплаты"}, "color": {"field": "payment_type", "type": "nominal", "label": "Тип оплаты"}}, "options": {"grouping": "stacked"}},
    {"id": "threshold_reach", "title": "Сколько рефереров получили бы месяц в июле", "subtitle": "Только обычные 20%-е рефереры и новые первые оплаты друзей внутри июля.", "type": "bar", "dataset": "threshold_rows", "sourceId": "threshold_db", "encodings": {"x": {"field": "threshold", "type": "nominal", "label": "Порог"}, "y": {"field": "july_qualifiers", "type": "quantitative", "label": "Рефереры с наградой"}}},
]

tables = [
    {"id": "variants", "title": "Сравнение механик", "subtitle": "Стоимость — дополнительный июльский baseline subsidy до доказанного прироста; $1.56–$3.37 за бесплатный сервис-месяц.", "dataset": "variant_rows", "sourceId": "variant_model", "defaultSort": {"field": "priority", "direction": "asc"}, "columns": [{"field": "priority", "label": "#", "format": "number"}, {"field": "model", "label": "Модель"}, {"field": "headline", "label": "Сила оффера"}, {"field": "july_service_months", "label": "Бесплатных месяцев", "format": "number"}, {"field": "added_cost_low", "label": "Риск, low", "format": "currency"}, {"field": "added_cost_high", "label": "Риск, high", "format": "currency"}, {"field": "cannibalization", "label": "Каннибализация"}, {"field": "fraud", "label": "Fraud"}, {"field": "decision", "label": "Решение"}]},
    {"id": "plan", "title": "Пошаговый запуск", "subtitle": "Один оффер, одна контрольная группа, одна ограниченная награда.", "dataset": "plan_rows", "sourceId": "plan_model", "defaultSort": {"field": "order", "direction": "asc"}, "columns": [{"field": "order", "label": "#", "format": "number"}, {"field": "when", "label": "Когда"}, {"field": "action", "label": "Что сделать"}, {"field": "owner_metric", "label": "Условие выхода"}]},
]

source_map = {source["id"]: source for source in sources}
for item in charts + tables:
    item["source"] = source_map[item["sourceId"]]

blocks = [
    {"id": "title", "type": "markdown", "body": "# DoodleVPN: безопасный реферальный толчок"},
    {"id": "executive", "type": "markdown", "body": "## Executive Summary\n\n- **Твоя тревога подтверждается базой.** В июльские семь бесплатных дней оплаты упали 147 → 56 (−61,9%), повторные — 118 → 37 (−68,6%). В мае после +3 дней повторные оплаты тоже упали 25 → 14 (−44%).\n- **Поэтому месяц за одного друга и дополнительные дни самому приглашённому отвергаю.** Первая схема создала бы 51 бесплатный месяц уже на июльском baseline; вторая напрямую отодвигает первое продление нового клиента.\n- **Конкретное решение: 30-дневный ограниченный тест `2 новых оплативших друга → 30 дней рефереру` поверх текущих 20%.** Только новые первые оплаты после старта, одна награда на реферера, 48-часовой hold, без ретроактива, special partners исключены.\n- **Риск ограничен.** На июльском потоке порог 2 дал бы 6 бесплатных месяцев, то есть $9,36–$20,22 дополнительной стоимости, против 51 месяца и $79,56–$171,87 у порога 1."},
    {"id": "evidence", "type": "markdown", "body": "## Бесплатные дни уже дважды останавливали платёжный импульс\n\nИюльская раздача была 16 июля: уведомление о +7 днях успешно ушло 4 707 пользователям. За точные семь дней до неё прошло 147 оплат от 141 плательщика; во время — 56 от 54. За первые 23,1 часа после окончания подарка, ещё до массовой рассылки скидки, прошло 30 оплат — против 56 за все 168 часов подарочного окна.\n\nМайский эпизод слабее, но совпадает по направлению. Это не чистый A/B: обе компенсации шли рядом с техработами, а post-окно июля содержит скидку 37%. Поэтому нельзя заявлять, что подарок причинил ровно −61,9%. Но для решения о риске две независимые временные реакции достаточны: массовый free access нельзя использовать как acquisition-механику."},
    {"id": "gift_chart", "type": "chart", "chartId": "gift_windows"},
    {"id": "threshold", "type": "markdown", "body": "## Порог два — единственная точка, где оффер сильный, а касса не ломается\n\nВ июле 51 обычный реферер привёл хотя бы одного нового плательщика, только 6 привели двоих и никто — троих. Поэтому `1→30` слишком широко субсидирует baseline, а `3→30` почти никто не почувствует. `2→30` создаёт достижимый прогресс `0/2 → 1/2 → 2/2` и при текущем объёме затрагивает лишь около 0,5% активной платящей базы.\n\nИсторически после 13 апреля до второго платного друга дошли 47 из 183 обычных рефереров с платными друзьями (25,7%), до третьего — 20 (10,9%). Порог два не гарантирует рост, но даёт лучший шанс с ограниченным downside."},
    {"id": "threshold_chart", "type": "chart", "chartId": "threshold_reach"},
    {"id": "variants_md", "type": "markdown", "body": "## Почему остальные модели проигрывают\n\n`14 дней за одного` — рабочий fallback, но на июльском baseline раздал бы 26,6 сервис-месяца: больше риска при менее сильном заголовке. `7 дней за одного` дешевле, но прошлые массовые подарки делают саму единицу награды психологически обычной, а не событийной. Игры, лотереи и кейсы пока не отвечают на главный вопрос — даёт ли сильный детерминированный оффер дополнительные первые оплаты."},
    {"id": "variants_table", "type": "table", "tableId": "variants"},
    {"id": "offer", "type": "markdown", "body": "## Точный оффер и UX\n\n**Главный экран treatment:**\n\n> 🎁 Месяц VPN за двух друзей\n>\n> Пригласи 2 друзей. Когда оба впервые оплатят подписку, получишь +30 дней.\n>\n> Ещё ты получаешь 20% с их оплат. Баланс от $1 можно тратить на VPN, от $10 — вывести в USDT.\n\nКнопки: **`Пригласить друга`** и **`0 из 2 оплатили`**. Мелко: `Другу доступны обычные 3 бесплатных дня. Награда после 48 часов проверки. Акция до {date}.`\n\nПосле первой подтверждённой оплаты: `🔥 1 из 2. Остался один друг — и месяц твой.` После второй и hold: `Готово: +30 дней начислены до {date}.` Никаких ежедневных напоминаний; только одно пользователю со статусом 1/2 через семь дней."},
    {"id": "economics", "type": "markdown", "body": "## Ограничитель потерь и критерий прибыли\n\nСервис-месяц считаем диапазоном: $1,32 внутренней ценности по лучшей дневной ставке + $0,24 инфраструктуры = $1,56 low; либо полный месячный net $3,13 + $0,24 = $3,37 high. На июльском baseline шесть наград стоят $9,36–$20,22.\n\nОграничить кампанию **30 наградами**: максимум 900 бесплатных дней и $101,10 high-risk exposure. Это потолок, не ожидаемый расход. При средней 30-дневной выручке обычного реферального плательщика $6,98 после 20% и $0,24 сервера один дополнительный плательщик даёт около $5,34 contribution до bonus. Масштабировать только когда контроль показывает не менее 15 дополнительных первых оплат и contribution остаётся положительным после списания всех выданных месяцев по high-сценарию."},
    {"id": "experiment", "type": "markdown", "body": "## Эксперимент даёт причинный ответ, а не красивую корреляцию\n\nОдинаково исправить referral UX в обеих группах, затем навсегда закрепить ordinary active payers: **70% treatment / 30% control**. Control сохраняет нынешние 20%; treatment получает только новый milestone. Primary metric: первые оплаты прямых друзей на 100 eligible referrers.\n\nНа 30-й день: `incremental paid = paid_T − eligible_T / eligible_C × paid_C`. Rollout: ≥15 incremental paid, rate lift ≥35%, incremental contribution >0, fraud/refund ≤5%, рост bot blocks <0,5 п.п. Kill: incremental ≤0 или contribution ≤0. Если знак положительный, но выборки мало — повторить второй 30-дневный сезон, не выкатывать всем по настроению."},
    {"id": "plan_md", "type": "markdown", "body": "## Что делать по шагам\n\nНе нужно сначала строить Mini App. Нужны один экран, три состояния прогресса, пять событий и автоматическое начисление после hold. Игра может появиться позже только как оболочка уже доказанного оффера."},
    {"id": "plan_table", "type": "table", "tableId": "plan"},
    {"id": "dont", "type": "markdown", "body": "## Что сейчас не делать\n\n- Не давать месяц за одного друга.\n- Не давать дополнительные бесплатные дни приглашённому после оплаты.\n- Не считать регистрации или trial основанием награды.\n- Не считать одновременно активных друзей: это непонятный и исчезающий статус; считать первые подтверждённые оплаты.\n- Не начислять новую награду задним числом.\n- Не смешивать special partners с массовой программой.\n- Не запускать игру, кейсы, рулетку или розыгрыш вместе с оффером.\n- Не рассылать кампанию ежедневно и не менять одновременно цену/скидку."},
    {"id": "fallback", "type": "markdown", "body": "## Если 2→30 не сработает\n\nСледующий и только следующий тест — **14 дней за одну новую первую оплату**, также одной наградой на человека и с тем же control. Он слабее как событие и дороже на baseline, поэтому не является первым выбором. Если и он не даёт incremental first-paid, бесплатные дни как реферальная валюта закрываем; оставляем 20%, понятную трату баланса от $1 и ищем рост в другом acquisition-механизме."},
    {"id": "questions", "type": "markdown", "body": "## Что ещё нужно подтвердить\n\nГлавный неизвестный параметр — реальная вероятность, что реферер после первого платного друга дойдёт до второго именно из-за progress-offer. Её нельзя честно вывести из прошлого поведения. Также июльский grant не имеет отдельного журнала получателей: 4 707 — подтверждённые доставки уведомления, а не сохранённый ledger начисления. Эти пробелы не мешают ограниченному тесту, но запрещают выдавать модель за прогноз."},
    {"id": "caveats", "type": "markdown", "body": "## Caveats and assumptions\n\nВсе времена в базе хранятся в UTC; окна выровнены по точному timestamp события. Транзакции, а не только уникальные плательщики, используются для кассовой реакции; уникальные плательщики приведены рядом. Майская и июльская раздачи не были рандомизированы. Техработы могли независимо снижать доверие и оплаты. Июльское post-окно частично поддержано 37%-й скидкой, а данных после 30 июля ещё недостаточно для полного 30-дневного recovery. Поэтому вывод — `высокий риск каннибализации`, а не точная оценка causal loss."},
]

artifact = {
    "surface": "report",
    "manifest": {"version": 1, "surface": "report", "title": "DoodleVPN: безопасный реферальный толчок", "description": "Decision memo on free-day cannibalization and referral threshold design.", "generatedAt": "2026-07-31T00:30:00Z", "sources": sources, "charts": charts, "tables": tables, "blocks": blocks},
    "snapshot": {"version": 1, "generatedAt": "2026-07-31T00:30:00Z", "status": "ready", "datasets": {"gift_rows": gift_rows, "threshold_rows": threshold_rows, "variant_rows": variant_rows, "plan_rows": plan_rows}},
    "sources": sources,
}

(ROOT / "artifact.json").write_text(json.dumps(artifact, ensure_ascii=False, indent=2), encoding="utf-8")
print(ROOT / "artifact.json")
