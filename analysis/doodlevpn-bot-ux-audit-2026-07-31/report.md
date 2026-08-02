# UX-аудит Telegram-бота DoodleVPN

Дата: 31 июля 2026

Объект: `Simpix/doodlevpn`, production-компонент `bot-go`

Проверенный [production-коммит](https://github.com/Simpix/doodlevpn/commit/ed6092ab0051d9f73d663eae9e9ad386ce0c6c96): `ed6092ab0051d9f73d663eae9e9ad386ce0c6c96`

Режим: read-only

## Метод и ограничения

Аудит восстановлен по точному коммиту, запущенному в production, production-конфигурации без секретов и обезличенным агрегатам SQLite. По прямому указанию владельца вход в Telegram не выполнялся. Поэтому фактическая логика экранов, кнопок и состояний проверена по коду, но визуальное поведение клиента Telegram и качественное понимание текстов реальными пользователями не наблюдались.

Срез базы сделан около 31.07.2026 02:13 МСК. События `user_events` существуют только с 08.06.2026 и не содержат `session_id` или `flow_id`: все 47 950 записей имеют эти поля пустыми. Поэтому 30-дневные числа ниже — направленные пользовательские срезы, а не строгая последовательная воронка.

В production ничего не менялось. Код не редактировался. Идентификаторы пользователей, usernames, кошельки, платёжные идентификаторы и секреты в отчёт не включены.

---

## 1. Executive verdict

**Да, текущий UX ограничивает рост — прежде всего реферальную активацию и использование уже заработанного баланса. Доказательств, что основной платёжный checkout является главным ограничителем роста, нет.**

Главное различие:

- экономика рефералки уже работает: 3 512 приглашённых регистраций, 823 приглашённых плательщика, 40 успешных продлений бонусами на 231,80 USDT и 11 исторических пользователей вывода;
- интерфейс не показывает полный жизненный цикл награды: нет сообщения о появлении hold, нет сообщения о переходе денег в available, нет прогресса конкретного друга;
- ценность доступного баланса объяснена неоднозначно: экран называет только порог вывода 10 USDT, хотя продление доступно от 1 USDT; 297 из 315 пользователей с положительным балансом не могут вывести деньги, но 239 уже могут продлить VPN;
- ранняя часть реферальной воронки практически не измеряется: открытие раздела, нажатие Share и открытие native Share отсутствуют в событиях;
- исторический «A/B-тест» текста Share не измерял Share или приглашённых друзей: пользователей делили по хэшу и сравнивали, платил ли сам пользователь. Выбор `share_v4` нельзя считать доказанным победителем реферальной конверсии.

Платёжный путь выглядит существенно здоровее:

- за 30 дней: 694 пользователя открыли тарифы, 569 выбрали тариф, 549 создали счёт, 547 открыли платёжную ссылку, 506 имели событие `paid`;
- это не sessionized funnel, но потери между созданием счёта и `paid` не выглядят главным провалом;
- из 2 656 канонических оплат 1 123 являются повторными, их совершили 708 пользователей;
- слабое место оплаты — не сам выбор тарифа, а отсутствие включённых payment-rescue уведомлений для незавершённых и неуспешных попыток.

### Решение владельцу

Первым менять **экран «Бонусы за друзей»**, а не главное меню и не checkout:

1. оставить один главный CTA — `🤝 Отправить приглашение`;
2. сразу объяснить: друг получает 3 дня, реферер получает процент после оплаты, деньги доступны через 48 часов;
3. раздельно показать `Доступно` и `В ожидании`;
4. явно написать: `от 1 USDT — продление VPN; 10 USDT — только вывод`;
5. контекстно показывать действие с балансом;
6. сгруппировать вывод и партнёрские инструменты, не удаляя их;
7. одновременно добавить минимальные события и уведомления `reward_pending` / `reward_available`;
8. mixed checkout пока не строить.

Рекомендуемый вариант — **вариант 2: перестройка иерархии без удаления функций**. Он исправляет доказанные проблемы понимания и приоритета, не требует сначала строить новый referral hub и не вмешивается в платёжную бухгалтерию.

---

## 2. Verified facts / assumptions / unknowns

### 2.1. Проверенные факты продукта

| Факт | Доказательство |
|---|---|
| Массовая комиссия по умолчанию — 20% | `users.ref_commission_pct`, default 20; `CreditReferrer` |
| Друг закрепляется за первым реферером | `UpsertUser`: `referrer_id=COALESCE(users.referrer_id, excluded.referrer_id)` |
| Hold по production-конфигурации — 48 часов | `REFERRAL_HOLD_HOURS`, production default |
| Вознаграждение создаётся только после paid/completed платежа | `CreditReferrer` проверяет канонический `payments.status` |
| Refund отменяет hold или формирует возврат/долг | `reverseReferralHoldsForRefund` |
| Доступный hold автоматически прибавляется к балансу | `ReleaseEligibleReferralHolds` |
| Пользователь не получает уведомление о создании hold | После `CreditReferrer` пользовательское сообщение отсутствует |
| Пользователь не получает уведомление о release | `runJobs` получает IDs от `ReleaseEligibleReferralHolds`, но только пишет количество в server log |
| Вывод доступен от 10 USDT | `refMinUSDT = 10` |
| Выводит весь доступный баланс | В state фиксируется весь `RefUSDTBalance`, затем он резервируется |
| Сети вывода — BEP20 и TON | `withdraw_net_bep20`, `withdraw_net_ton` |
| Продление бонусами доступно от 1 USDT | `refSpendSubMinBalance = 1.0` |
| Продление не требует полной оплаты выбранного тарифа | Весь баланс переводится в целое число дней по лучшей дневной ставке |
| Mixed checkout не поддержан | Нет заказа «бонусный debit + provider remainder» |
| Дробные дни не поддержаны | Число дней округляется вниз до целого |
| Лучший курс берётся от годового тарифа | Минимальный `PriceUSD / Days`: 22,99 / 365 |
| Расчётная цена одного дня после скидки — около 0,0441 USDT | `22.99 × 0.70 / 365` |
| Значит, 0,01 USDT не может дать даже один день | Текущая модель целых дней |
| Telegram-экран показывает только агрегаты | `RefStats`: registrations, paid, hold, total |
| По каждому другу данные уже есть в backend | `ReferralInvites`: label, created_at, paid, earned, hold; web API их отдаёт |
| Первый экран нового пользователя не содержит рефералку | В `MainMenuWithAntiTraffic` ветка `firstTime` делает ранний `return` |
| После успешной настройки есть контекстная кнопка приглашения | `GuideDoneKB` |
| После покупки есть invite-напоминания | 48 часов, 1 неделя, 3 недели; 1w/3w подтверждены production-логом |
| Payment rescue в production выключен | `PAYMENT_RESCUE_NOTIFIERS_ENABLED` не задан, default false |
| Referral outreach в production выключен | `REFERRAL_OUTREACH_NOTIFIERS_ENABLED` не задан, default false |
| Lifecycle setup rescue в production выключен | `LIFECYCLE_RESCUE_NOTIFIERS_ENABLED` не задан, default false |
| Expiry/winback подписки работает отдельно от lifecycle-rescue flag | `expirySpecs` включены без этого флага |
| Публичное упрощённое меню подписки включено | `PROFILE_MENU_PUBLIC_ENABLED=true` |
| Trial — 3 дня, лимит устройств — 5 | production-конфигурация |

### 2.2. Проверенные агрегаты

#### Пользователи и подписка

| Состояние | Пользователи |
|---|---:|
| Всего | 6 496 |
| Trial не использован | 918 |
| Trial использован | 5 578 |
| Активная подписка любого типа | 1 230 |
| Активные плательщики | 1 145 |
| Активный trial / неоплаченный доступ | 85 |
| Когда-либо платили | 1 533 |
| Бывшие плательщики с истёкшей подпиской | 388 |
| Истёкший trial, никогда не платили | 4 220 |

#### Реферальный канал

| Показатель | Значение |
|---|---:|
| Зарегистрировано по рефералу за всё время | 3 512 |
| Из них активировали trial | 3 093, или 88,1% |
| Из них когда-либо заплатили | 823, или 23,4% от регистраций |
| Рефереров хотя бы с одной регистрацией | 609 |
| Рефереры только с 1 другом | 338 |
| Рефереры с 2 друзьями | 103 |
| Рефереры с 3–5 друзьями | 107 |
| Рефереры с 6+ друзьями | 61 |
| Остановились на 1–2 друзьях | 441 из 609, или 72,4% |

823 из 1 533 когда-либо плативших пользователей имеют реферера — 53,7%. Это показывает масштаб канала, но **не является оценкой causal uplift**, потому что нет рандомизированного контроля «без реферала».

#### Баланс

| Текущий баланс | Пользователи | Доля от 315 |
|---|---:|---:|
| Любой положительный | 315 | 100% |
| 0,01–0,99 USDT | 76 | 24,1% |
| 1,00–2,09 USDT | 107 | 34,0% |
| 2,10–9,99 USDT | 114 | 36,2% |
| 10+ USDT | 18 | 5,7% |
| Ниже порога вывода | 297 | 94,3% |
| Уже могут продлить VPN от 1 USDT | 239 | 75,9% |

Суммарный текущий положительный баланс — 1 112,24 USDT.

#### Начисление, трата и вывод

| Операция | Объём |
|---|---:|
| Direct reward со статусом available | 1 409 начислений, 317 рефереров, 807 друзей, 3 124,70 USDT |
| Direct reward сейчас в hold | 11 начислений, 12,05 USDT |
| Успешное продление бонусами | 40 операций, 30 пользователей, 231,80 USDT |
| Минимальное успешное списание | 1,01 USDT |
| Максимальное успешное списание | 123,78 USDT |
| Исторически завершённый вывод | 17 заявок, 11 пользователей, 650,35 USDT |

30 пользователей, тративших баланс, нельзя корректно делить на текущие 315 положительных балансов как на одну когорту: числитель исторический, знаменатель моментальный. Соотношение 9,5% можно использовать только как грубый сигнал низкого охвата, не как продуктовую конверсию.

#### Платёжный путь

| Уникальные пользователи за 30 дней | Число |
|---|---:|
| Открыли тарифы | 694 |
| Выбрали тариф | 569 |
| Создали счёт | 549 |
| Открыли платёжную ссылку | 547 |
| Получили `paid` | 506 |
| Отметили завершение настройки | 312 |

В таблице provider attempts накоплено:

- карта/СБП: 2 579 paid, 756 expired, 701 pending, 71 failed, 3 refunded;
- crypto: 258 paid, 139 expired, 270 pending, 15 failed.

Это не provider conversion: один человек может создавать несколько попыток, а старые `pending` могут быть фактически брошенными. Но массив незавершённых попыток существует, тогда как rescue-функция выключена.

### 2.3. Предположения, которые нельзя объявлять фактами

| Предположение | Статус |
|---|---|
| Пользователи считают 10 USDT порогом для любого использования | Правдоподобно по тексту, но не измерено |
| Слово USDT снижает доверие | Не измерено |
| `Я инфлюенсер` создаёт ощущение MLM | Экспертная оценка риска, не пользовательский факт |
| Пользователь не замечает кнопку Share | Не измерено |
| Прогресс друзей увеличит второе приглашение | Гипотеза для теста |
| Mixed checkout повысит оплату | Не доказано |
| Реферальная кнопка нужна на первом экране нового пользователя | Не доказано; раннее приглашение может отвлечь от trial |
| Текст `share_v4` лучше остальных | Не доказано корректным экспериментом |

### 2.4. Неизвестно

- число показов главного меню и реферальной кнопки;
- число открытий реферального экрана;
- число кликов по URL-кнопке Share;
- число реально отправленных сообщений — Telegram этого не подтверждает;
- число ручных копирований `<code>`-ссылки;
- понимание hold, порога 1 USDT и порога 10 USDT;
- причины, почему 72,4% рефереров останавливаются на 1–2 друзьях;
- причины отказа после provider attempt;
- support-категории и обращения по рефералке: support-источник не исследовался;
- качество визуального рендера длинных expandable blockquotes в конкретных версиях Telegram;
- сколько пользователей пришли через чужую пересылку ссылки, а не через встроенную Share-кнопку.

---

## 3. Полная карта экранов, кнопок и функций

Обозначения использования:

- число — найденный production-счётчик;
- `не измеряется` — события нет;
- `URL не измеряется` — Telegram открывает внешний URL без callback;
- `состояние, не действие` — количество пользователей в состоянии есть, нажатия нет.

### 3.1. Команды и входы

| Экран / команда | Что видит и кому | Кнопки / callback | Следующий экран / назад | Backend | Использование | Назначение / риск потери |
|---|---|---|---|---|---:|---|
| `/start [payload]` | Все; welcome, trial suffix по состоянию | Trial, Buy/Extend, Profile, Referral, Lang, Help, Legal; состав зависит от state | Основные флоу; возврат обычно `start` | attribution parser, user upsert, channel gate, subscription refresh | 3 153 событий с 08.06; 1 668 за 30 дней | Главный роутер; критично |
| `/start ref_*` | Приглашённый пользователь | То же меню после атрибуции | Trial / buy | permanent `referrer_id`, `start_attributions` | 704 ref-touches, 453 пользователя в окне событий | Реферальная атрибуция; критично |
| `/profile` | Любой пользователь | State-specific profile keyboard | Setup, devices, buy, start | subsvc/Remnawave status | не измеряется | Self-service подписки; критично |
| `/invite` | Любой пользователь | Referral keyboard | Share/spend/withdraw/partner/start | `RefStats` | не измеряется | Альтернативный вход в рефералку |
| `/app` | Пользователь с доступом к mobile app code | Код / инструкции | App flow | mobile app code | не измеряется | Подключение приложения |
| `/support` | Любой | Только текст с username поддержки | Внешний Telegram contact | нет | не измеряется | Support fallback |
| `/language` | Любой | RU / EN / Back | Перерисовка main menu | `SetLang` | не измеряется | Локализация |

Административные команды и callbacks присутствуют, но не включены в consumer IA. Они не должны смешиваться с пользовательским меню.

### 3.2. Первый вход, trial и главное меню

| Экран | Текст / состояние | Кнопки | Условие видимости | Следующий / назад | Backend | Использование | Назначение / риск |
|---|---|---|---|---|---|---:|---|
| Channel gate | Требование подписаться на `@doodlemedia` | URL канала; `check_channel_sub` | Все не-admin до основных экранов | После проверки — нужный landing/main | Telegram membership | не измеряется | Acquisition/owned media; потеря блокирует весь trial |
| Первый main menu | Welcome + 3 дня бесплатно | `trial`, `buy`, language; иногда `infl_sub_request` | `existingUser == nil` | Trial / plans / lang | user + active-sub check | не измеряется | Фокус на activation; referral/help/legal намеренно не показаны |
| Returning pretrial | Welcome + trial | `trial`, `buy`, referral, lang, help, legal | Trial не использован, нет оплаты, уже существующий user | Соответствующие разделы | `welcomeFlags` | 918 пользователей state | Активация trial |
| Active trial / paid main | Welcome | `guide`, `my_vpn`, referral, lang, help, legal | Активная подписка | Setup/profile/etc. | live subscription state | 1 230 пользователей state | Использование VPN и self-service |
| Inactive returning main | Welcome | `buy`, referral, lang, help, legal | Нет active, не first-time | Plans etc. | subscription state | 4 954 expired state | Возврат/продление |
| Trial activation | 3 дня, ключ/лимит устройств | platform guide | Trial ещё не использован, не paid, risk passes, channel joined | Guide; назад через guide main | `EnsureUser`, `SetTrialUsed` | 561 событий с 08.06; 275 за 30 дней | Activation; критично |
| Trial already used / blocked | Telegram alert | Нет нового экрана | Повтор / risk state | Остаётся на месте | risk + trial flags | не измеряется | Anti-abuse |

Риск первого меню не в количестве кнопок: одновременно требуется решить только `trial` или `buy`. Не доказано, что туда следует добавлять рефералку. Проблема — отсутствие события показа и невозможность проверить timing.

### 3.3. Подписка и подключение

| Экран | Кнопки / callback | Кому / условие | Следующий / назад | Backend | Использование | Назначение / риск |
|---|---|---|---|---|---:|---|
| No subscription profile | `trial`, `buy`, `start` | Нет записи active sub | Trial/plans/main | subsvc/Remnawave | состояние не измеряется отдельно | Recovery |
| Expired profile | `buy`, `start` | Expired | Plans/main | subscription status | 4 954 expired any | Renewal; критично |
| Active condensed profile | `guide`, `my_devices`, optional anti-traffic, `buy`, optional `rotate_key`, `start` | Public profile flag + active | Setup/devices/etc. | subsvc, devices, expiry | не измеряется | Self-service; критично |
| Manual connection | `app_code`, `sub_qr`, `guide`, `start` | Condensed profile + active URL | App/QR/platform | subscription URL | не измеряется | Advanced setup |
| App code | App-login code flow | Feature enabled, active state | App | app-code backend | не измеряется | Mobile app login |
| QR | QR + back | Active URL | `my_vpn` | QR generation | не измеряется | Setup |
| Rotate key confirm | `rotate_key_confirm`, `my_vpn` | subsvc enabled | Success/profile | subsvc rotate | не измеряется | Security/recovery; высокий риск |
| Devices | Device rows/delete, `buy_devices`, `my_vpn` | Active | Delete/top-up/profile | subsvc devices | не измеряется | Device management |
| Buy devices | Quantity/custom and provider callbacks | Active | Payment; back | device purchases | отдельные provider rows, CTA не измеряется | Add-on revenue |
| Anti-traffic | Packages/custom and provider callbacks | Quota feature applicable | Payment/profile | subsvc quota | 4 paid add-on rows в канонических payments | Niche add-on; не удалять без usage window |

### 3.4. Guide / установка

| Экран | Кнопки / callback | Условие | Следующий / назад | Backend | Использование | Назначение / риск |
|---|---|---|---|---|---:|---|
| Platform picker | iOS, Android, Windows, macOS, Linux, Router, TV; optional Manual; `start` | После trial/payment/profile | Unified guide / router / TV | sub URL | не измеряется | Основной activation |
| Unified mobile/desktop guide | Download URL; one-click import; next/back | Active sub URL | App, later guide step | deep-link bridge | URL не измеряется | Setup |
| TV guide | Happ TV link; later steps | TV | TV setup / back | TV code/link | не измеряется | TV setup |
| Router hub | OpenWrt, Keenetic, AsusWRT, Linux/mihomo; back | Router | Device-specific instructions | setup link handler | не измеряется | Router setup |
| OpenWrt hub | OpenClash, Nikki, Passwall, homeproxy, v2rayA; back | OpenWrt | App-specific instructions | subscription URL | не измеряется | Advanced setup |
| Setup troubleshooting | Download/key/no internet/support | Setup failure | Guide/FAQ/support | FAQ/setup handlers | не измеряется | Activation rescue |
| Guide done | Invite friend URL; `start` | Пользователь нажал «Готово» | Native Share / main | `guide_connected` | 647 событий всего, 442 за 30 дней; 431 / 312 уникальных пользователей | Activation confirmation + contextual referral |

`guide_connected` является самоотчётом по нажатию «Готово», а не техническим подтверждением VPN-трафика.

### 3.5. Help, legal, language

| Экран | Кнопки | Следующий / назад | Использование | Назначение / замечание |
|---|---|---|---:|---|
| Help | 4 FAQ; back | FAQ / main | не измеряется | Текст показывает `@doodlevpn_support`, но отдельной URL-кнопки поддержки нет |
| FAQ connect/speed/apps/server | `my_vpn`; `help` | Profile / help | не измеряется | Диагностика; `my_vpn` может привести в expired/no-sub state |
| Legal | Terms, Privacy, recurring offer; `start` | Web URLs / main | URL не измеряется | Compliance; не удалять |
| Language | RU, EN, start | Main menu | не измеряется | Меняет сохранённый язык |

Кнопка главного меню называется «Поддержка», но callback `help` открывает FAQ. Прямой контакт есть текстом, команда `/support` выдаёт username, а URL-кнопки на этом экране нет. Это не тупик, но название обещает более прямое действие, чем выполняет.

### 3.6. Оплата и продление

| Экран | Кнопки / callback | Условие | Следующий / назад | Backend | Использование | Назначение / риск |
|---|---|---|---|---|---:|---|
| Plans | `plan_1m`, `plan_3m`, `plan_6m`, `plan_1y`; `promo`; `gift`; `start` | Buy/extend | Payment method/promo/gift/main | config plans | 2 364 opens total; 1 256 за 30 дней | Revenue; критично |
| Promo input | Stateful text | Promo click/deep-link | Discounted plans | promo tables | 174 applied total; 130 за 30 дней | Conversion campaign |
| Payment method | `pay_wata_*`, `pay_crypto_*`, `pay_stars_*`, `buy` | Plan selected | Provider invoice / plans | provider clients | 2 002 selected total; 1 070 за 30 дней | Revenue; критично |
| Card/SBP invoice | URL pay, `check_wata_*`, `plan_*` | Invoice created | External payment/check/back | WATA + redirect tracking | 2 579 paid attempts; click tracked | Revenue |
| Crypto invoice | URL pay, `check_crypto_*`, `plan_*` | Invoice created | External payment/check/back | Heleket | 258 paid attempts | Revenue |
| Stars invoice | Native invoice / back | Stars selected | Telegram checkout | Stars | входит в canonical payments | Revenue |
| Pending check | Pending copy + check/back | Provider pending | Same screen / success / error | poll/webhook | 1 603 checks total; 857 за 30 дней | Status recovery |
| Failed/expired provider state | Retry/other method depending on path | Failed/expired | New attempt / method | normalized provider status | 86 failed и 895 expired provider attempts | Recovery |
| Payment success | Platform picker + main | Fulfilled payment | Setup/main | canonical payment + EnsureUser | 2 656 canonical paid rows | Activation |
| Gift username | Text input + back | Gift selected | Gift plans | user lookup | не измеряется | Gift revenue |
| Gift plans/method/payment | Plans, same three providers, back | Valid recipient | Provider flow | recipient fulfillment | usage отдельно не сведено | Gift revenue; не удалять |

### 3.7. Lifecycle уведомления

| Сообщение | CTA | Аудитория / production | Использование | Назначение / риск |
|---|---|---|---:|---|
| Trial T−3d/T−1d/T−1h | Buy / setup help | Never-paid expiring | 1 070 отправок, 565 пользователей по группе | Convert trial |
| Paid T−3d/T−1d/T−1h | Buy | Paid expiring | 933 отправки, 430 пользователей по группе | Renewal |
| Trial T+1d winback | Buy / help | Never-paid expired | 816 | Winback |
| Paid T+1d winback | One-time promo | Former payer | 355 | Winback |
| Invite friend 48h/1w/3w | `referral` | После первой оплаты | 1w: 670; 3w: 812 | Referral activation |
| Payment no-click/unpaid/expired rescue | Pay/check/other method | Provider attempts | **выключено в production** | Recover failed checkout |
| Referral balance T−7d/T−2d | `ref_spend_sub` | Expiring sub, balance 2,10–20 | 13 и 17 отправок | Balance utilization |

Название `ref_balance_expiry_*` вводит разработчика в заблуждение: истекает подписка, а не реферальный баланс.

### 3.8. Реферальный раздел

| Экран / функция | Текущий текст и кнопки | Кому / условие | Следующий / назад | Backend | Использование | Назначение / риск |
|---|---|---|---|---|---:|---|
| Referral main | 20%, available balance, raw link, registrations, paid, hold, total, terms | Любой | Далее по кнопкам; `start` | `RefStats`, user commission | открытие не измеряется | Центральный referral hub |
| Invite friend | `t.me/share/url` с `share_v4` | Любой | Native Share; Telegram не возвращает callback | ref deep-link | URL не измеряется | Referral acquisition; критично |
| Raw link | `<code>…ref_*` | Любой | Ручное копирование | attribution parser | не измеряется | Альтернатива Share |
| “Я инфлюенсер” | Partner offer + contact URL | Любой | Contact / `referral` | нет user mutation | не измеряется | Partner acquisition |
| Custom ref | Text input | Только special partner | Referral | `SetCustomRefCode`, aliases | не измеряется | Partner operations |
| Invite influencer | Username/TG ID input | commission ≥50% или master | Creates/attaches influencer | master/ref backend | не измеряется | Second-level partner network; критично для 41 special users |
| Request annual sub | Source input, admin approval | Eligible influencer | Admin review | influencer request | не измеряется | Partner benefit/control |
| Spend balance | Alert if <1; otherwise confirmation | Любой; backend min 1 | Confirm / referral | `StartRefSpend`, `EnsureUser`, complete/refund | 40 completed ops, 30 users | Balance utility; критично |
| Spend confirm | Cost, days, remainder; confirm/back | Balance ≥1 | Success/referral | atomic ledger | success aggregate exists, click нет | Financial action |
| Withdraw | Alert if <10, otherwise network picker | Любой; backend min 10 | Network/wallet/referral | reserve + request | 17 completed, 11 users historical | Payout; compliance/finance critical |
| Withdraw network | BEP20, TON, back | Balance ≥10 | Wallet input | validators | не измеряется | Payout |
| Withdrawal result | Submitted/approved/rejected | Request state | No persistent history screen in bot | admin workflow + refund | requests measured, opens нет | Trust/operations |
| Master stats block | Team influencers, registrations, paid, earned/hold | Master only | Same referral screen | master aggregates | 8 master users | Partner operations |

### 3.9. Мёртвые элементы, дубли, тупики и неожиданные возвраты

1. **Мёртвые UI helpers:** `RefSpendPlansKB` и `RefSpendConfirmKB` нигде не вызываются. Они генерируют `ref_buy_*` и `ref_confirm_*`, для которых нет зарегистрированных consumer handlers. Пользователь их сейчас не видит, поэтому это мёртвый код, а не сломанная живая кнопка.
2. **Два способа приглашения — не обязательно дубль:** referral main и guide done используют одинаковый native Share, но второй является контекстным моментом после успешной настройки.
3. **Историческая Share A/B-статистика не соответствует названию:** она сравнивает payment status самого assigned user, а не share/referral outcome.
4. **Help не ведёт напрямую в support:** визуальная кнопка «Поддержка» открывает FAQ; прямой username находится в тексте.
5. **First-time early return:** referral/help/legal отсутствуют только на первом main menu. Это может быть полезным progressive disclosure, но сейчас эффект не измеряется.
6. **Reward lifecycle — функциональный тупик обратной связи:** backend меняет hold → available, но пользователь узнает об этом только при ручном возвращении в referral main.
7. **Spend <1 — тупик без прогресса:** кнопка видна, но возвращает общий alert «заработай больше», не называет порог и недостающую сумму.
8. **Withdrawal history отсутствует в Telegram:** после заявки нет экрана для повторной проверки статуса. Web API имеет payout history, Telegram — нет.
9. **Payment rescue код существует, но выключен:** пользователь после брошенной/неуспешной оплаты не получает предусмотренный recovery.
10. **Устаревший notifier:** referral-balance nudge отбирает только 2,10–20 USDT, хотя текущий spend handler работает от 1 USDT и выдаёт пропорциональные дни.

---

## 4. Карта пользовательских состояний

Количество нажатий указано от первого видимого экрана состояния и не включает внешний платёжный интерфейс. Channel gate добавляет минимум два действия: открыть канал и проверить подписку.

| Состояние | Что видит первым | Визуально главное | Бизнес-цель | Совпадение | Что непонятно | Нажатия до результата / риск «функции нет» |
|---|---|---|---|---|---|---|
| Новый | Welcome, 3 дня; Trial, Buy, Language | Trial | Trial activation | Да | Referral/help/legal не видны | Trial: 1; с channel gate: 3. Referral может казаться отсутствующей |
| Существующий до trial | Welcome + полный returning menu | Trial | Trial | Да | Неясно, зачем Buy до пробы, но выбор понятен | Trial: 1 |
| Активный trial | Main с Connect/Profile | Connect | Реально настроить VPN | Да | Технический success не проверяется | Guide: 1; импорт: ещё 1–2 |
| Trial заканчивается | Push T−3d/1d/1h | Продлить | Первая оплата | Да | На 1d есть setup help, на 3d/1h меньше диагностики | Plans: 1; invoice: 3 |
| Активный плательщик | Main с Connect/Profile | Connect / profile | Использование и удержание | Да | Referral balance не интегрирован в profile | Profile: 1; extend: 2 |
| Перед продлением | Paid expiry push | Продлить | Renewal | Да | Наличие bonus balance не упоминается | Invoice: 3–4; balance spend не обнаруживается |
| Истёкшая подписка | Returning main с Extend либо expired profile | Extend | Renewal | Да | Если есть бонусы, они скрыты в отдельной рефералке | Plans: 1; paid invoice: 3 |
| Бывший плательщик T+1d | Winback promo | Promo extend | Reactivation | Да | Наличие баланса и выбор «promo или bonus days» не сопоставлены | Promo plans: 1 |
| После неуспешной оплаты | Provider screen при ручной проверке; позже обычно тишина | Check/retry | Recovery | Частично | Причина failure и лучший альтернативный метод | Payment rescue выключен; пользователь может решить, что платёж потерян |
| Без рефералов | Referral main с нулями, 20%, link | Share конкурирует с influencer/spend/withdraw | Первое приглашение | Частично | Друг получает 3 дня только в Share-тексте; spend/withdraw недоступны, но равноправны | Share из hub: 1; из main: 2 |
| Друг зарегистрирован, не платил | Только `Регистраций: 1`, `Оплат: 0` | Все те же CTA | Довести друга до trial/payment без давления | Нет явного действия | Начал ли trial, что дальше, можно ли помочь | Статус есть только на уровне агрегата; человек может решить, что атрибуция сломалась |
| Pending reward | `На холде (48ч): X` | Все те же CTA | Дождаться release / продолжить share | Частично | От какого друга, точная дата release, почему hold | Ручной вход: 2; push отсутствует |
| Available 0,01–0,99 | Balance виден; spend button visible | Spend выглядит доступным | Копить/пригласить ещё | Нет | Порог 1 USDT, недостающая сумма | Click даёт alert; результат невозможен. 76 пользователей |
| Available 1,00–2,09 | Balance; spend / withdraw / influencer | Несколько равных CTA | Использовать balance | Частично | Что покупается: тариф или дни; 10 USDT только для вывода? | Spend: 2 внутри hub; 3 из main. 107 пользователей |
| Available 2,10–9,99 | То же | Spend / withdraw | Spend | Частично | Можно ли доплатить; withdraw всё равно unavailable | Spend: 2. 114 пользователей |
| Balance 10+ | То же | Spend и withdraw равноправны | Spend либо payout | Да, но без guidance | Выводится весь баланс; история заявок отсутствует | Spend: 2; withdrawal submission: 3 + ввод. 18 пользователей |
| Обычный реферер | Общий referral screen | Share, influencer, spend, withdraw | Share/repeat share | Частично | Partner CTA не относится к большинству | 1 к Share; decision load выше необходимого |
| Special partner | Общий screen + custom/invite influencer/master block | Много операций одинакового веса | Управлять партнёрством | Частично | Разница direct и master hold объяснена техническим языком | Операции доступны; перенос без cohort rule опасен |

### Вывод по state matching

Проблема не в том, что в продукте «слишком много функций». Проблема в том, что один и тот же referral keyboard показывается состояниям от 0 до 100+ USDT:

- недоступные финансовые действия всё равно выглядят доступными;
- partner acquisition имеет тот же визуальный вес, что Share;
- available и pending меняют цифры, но не меняют главное действие;
- специальный партнёр действительно нуждается в дополнительных функциях, обычный пользователь — нет.

Это задача иерархии и контекстного показа, а не удаления функций.

---

## 5. Десять главных UX-проблем с доказательствами

### 1. Ранняя реферальная воронка не измеряется

**Severity: P0 для принятия решений.**

Нет `referral_screen_open`, `referral_share_click`, `referral_native_share_open`, `referral_link_copy`. Поэтому нельзя ответить, не видят ли кнопку, не понимают ли оффер или открывают Share, но не отправляют.

Следствие: любое крупное изменение referral IA до базовой телеметрии будет основано на впечатлении.

### 2. Backend меняет состояние награды молча

**Severity: P0 для доверия и использования баланса.**

После оплаты создаётся hold. Через 48 часов job переводит его в available и увеличивает баланс. IDs получателей возвращаются в `runJobs`, но используются только для server log. Нет ни `reward_pending`, ни `reward_available`, ни пользовательского сообщения.

Следствие: ценность программы не подкрепляется в момент, когда она реально возникла.

### 3. Два финансовых порога объяснены как один

**Severity: P0 для balance utilization.**

На экране написано только `Вывод в USDT от 10$`. Продление от 1 USDT не указано. Кнопка `Потратить на подписку (-30%)` не называет минимум.

Данные: 297 из 315 положительных балансов ниже 10 USDT, но 239 из них уже могут оплатить дни VPN.

Следствие: правдоподобно ложное ожидание «до $10 деньги бесполезны». Понимание не измерено, поэтому это риск, а не доказанное мнение пользователей.

### 4. Оффер не отвечает, что получает друг

**Severity: P1 для первого Share.**

Referral main говорит только: `получай 20% с каждой оплаты`. Три бесплатных дня для друга появляются лишь в тексте native Share после нажатия.

Следствие: за пять секунд пользователь видит собственную комиссию, но не взаимную пользу рекомендации. Это повышает риск ощущения рекламы/MLM.

### 5. Равный визуальный вес у действий для разных аудиторий

**Severity: P1.**

У обычного пользователя рядом стоят:

- пригласить друга;
- я инфлюенсер;
- потратить на подписку;
- вывести.

При этом special partner всего 41, а положительный баланс есть у 315 пользователей. Нельзя удалять partner flow, но для обычного пользователя он не должен конкурировать с основным CTA.

### 6. Не показан прогресс конкретного друга, хотя данные готовы

**Severity: P1 для повторного приглашения и доверия.**

Telegram показывает только `Регистраций / Оплат`. Backend уже имеет `ReferralInvites` с label, paid, earned, hold и использует это в web referral API.

Следствие: пользователь не может отличить «друг не начал trial», «не оплатил», «оплатил, reward pending», «reward available».

### 7. Маленький баланс приводит к общему отказу вместо прогресса

**Severity: P1 для 76 текущих пользователей.**

При 0,01–0,99 USDT кнопка видна. Alert: `Недостаточно бонусного баланса. Заработай больше — приглашай друзей!`

Он не называет:

- минимум 1 USDT;
- текущую сумму;
- сколько не хватает;
- почему 0,01 не может купить день.

Тон «заработай больше» также смещает продукт к промо-партнёрству вместо спокойной рекомендации знакомому.

### 8. Эксперимент Share имеет неверную outcome-метрику

**Severity: P0 для аналитической достоверности.**

`ABShareStats` хэширует всех пользователей по шести текстам и проверяет, платил ли **сам assigned user**. События показа текста, Share, ref-link open и referred payment не участвуют.

Следствие: hardcoded `share_v4` можно считать продуктовым решением, но не доказанным winner.

### 9. Referral-balance уведомление устарело относительно текущего продукта

**Severity: P1.**

Notifier:

- выбирает только 2,10–20 USDT;
- комментарий утверждает, что ниже 2,10 «нельзя купить ничего»;
- текущий handler работает от 1 USDT и выдаёт пропорциональные дни;
- текст называет USDT-баланс «подарочными рублями»;
- формулировка `заплатить на 30% меньше` может восприниматься как частичная оплата обычного тарифа, которой нет.

Фактический охват очень мал: 13 сообщений T−7d и 17 T−2d.

### 10. Failed-payment recovery предусмотрен, но выключен

**Severity: P1 для checkout recovery.**

В коде есть:

- no-click через 15 минут;
- unpaid через 30 минут;
- card expired/failed;
- crypto pending;
- стабильный 10% holdout.

Production flag выключен. При этом provider tables содержат сотни pending/expired попыток. Нельзя включать цепочку без очистки eligibility и baseline, но отсутствие recovery — реальный разрыв.

---

## 6. Путь оплаты и продления

### 6.1. Текущий путь

```text
Main / expiry notification
  → Тарифы
  → 1 / 3 / 6 / 12 месяцев
  → Карта/СБП | Криптовалюта | Telegram Stars
  → Счёт / внешняя оплата
  → Проверить оплату или webhook/poller
  → Активация/продление
  → Выбор платформы
```

Тарифы:

- 1 месяц — 249 ₽ / 2,99 USD;
- 3 месяца — 597 ₽ / 6,99 USD;
- 6 месяцев — 1 044 ₽ / 11,99 USD;
- 1 год — 1 992 ₽ / 22,99 USD.

Среди канонических paid rows:

- 1 месяц — 1 655;
- 3 месяца — 600;
- 6 месяцев — 225;
- 1 год — 141.

Короткий тариф доминирует, что ожидаемо для low-ticket VPN. Нельзя делать вывод о preference без учёта периода доступности тарифов и повторных оплат.

### 6.2. Что работает

- План и способ оплаты разделены по экранам.
- На каждом уровне есть back.
- Payment URL идёт через redirect и получает `payment_url_click`.
- Provider status нормализован.
- Fulfillment идемпотентен.
- Success сразу ведёт к platform setup.
- Renewal сохраняет ключ и объясняет это в lifecycle copy.

### 6.3. Что мешает

1. При наличии referral balance обычный checkout о нём не сообщает.
2. Пользователь не может понять выбор:
   - потратить баланс на несколько дней сейчас;
   - купить полный тариф обычным способом;
   - совместить оба — нельзя.
3. Payment rescue выключен.
4. `user_events` не sessionized: нельзя строго установить abandon point одного заказа.
5. Provider `pending` может жить долго и загрязнять операционный взгляд на failure.

### 6.4. Renewal с балансом

Текущий бонусный flow не покупает выбранный тариф. Он:

1. берёт весь доступный balance;
2. использует цену дня годового тарифа со скидкой 30%;
3. округляет число дней вниз;
4. списывает только стоимость целого числа дней;
5. оставляет копеечный остаток;
6. продлевает текущую или создаёт подписку через тот же `EnsureUser`.

Это проще и безопаснее mixed checkout, но текущий текст не объясняет эту механику до confirmation screen.

---

## 7. Полная реферальная воронка

| Шаг | Что должен понять пользователь | Что показано сейчас | Действие | Событие | Данные / разрыв |
|---|---|---|---|---|---|
| 1. Увидел возможность | Можно спокойно порекомендовать другу | `🤝 Пригласить друга` в returning menu, guide done, post-pay reminders | Открыть referral | Нет impression/click event | First-time menu её не показывает |
| 2. Открыл раздел | Другу польза, мне reward, условия | 20%, balance, link, aggregates, hold 48h, withdraw $10 | Решить отправить | Нет `referral_screen_open` | Open rate неизвестен |
| 3. Понял оффер | Другу 3 дня; мне % после оплаты | Три дня отсутствуют на hub | Share | Нет | Понимание неизвестно |
| 4. Открыл Share | Это ещё не отправка | Native Telegram share URL + `share_v4` | Выбрать чат/отправить | Нет | Нельзя называть открытие отправкой |
| 5. Друг открыл link | Попадёт в правильный bot/start | `ref_*` deep-link | `/start` | Частично: `start` + attribution | 704 touches / 453 users в окне с 08.06 |
| 6. Зарегистрировался | Атрибуция закреплена | Welcome/channel gate | Trial | Нет отдельного referred registration event | 3 512 all-time |
| 7. Активировал trial | 3 дня без карты | Trial + setup | Connect | `free_access_activated`, но без referral semantic event | 3 093, 88,1% registrations |
| 8. Первый раз оплатил | Reward появится у реферера | Обычный checkout | Pay | `paid`, но нет `referred_first_payment` | 823 all-time referred payers |
| 9. Reward pending | Сумма, источник, release date | У реферера ничего не pushится; на hub только общий hold | Ждать | Нет `reward_pending` | 11 текущих direct holds |
| 10. Reward available | Деньги можно использовать | Job молча увеличивает balance | Вернуться в hub | Нет `reward_available` | 1 409 available начислений |
| 11A. Потратил | От 1 USDT → точное число дней | Кнопка без threshold; confirm показывает дни | Confirm spend | Нет funnel events; ledger result есть | 40 операций / 30 users / 231,80 |
| 11B. Вывел | Только от 10; весь balance; сеть/срок | Threshold в terms; alert при недостатке | Network + wallet | Нет UX events; request state есть | 17 completed / 11 users |
| 12. Повторил | Видеть результат и следующий разумный шаг | Только aggregate stats | Share снова | Нет | 72,4% рефереров останавливаются на 1–2 |

### Где теряется измеримость

Воронка имеет хорошую backend-атрибуцию с шага 5 и финансовую истину с шага 8, но почти слепа на шагах 1–4 и 9–11. Поэтому нельзя локализовать причину низкой повторяемости:

- discovery;
- comprehension;
- share intent;
- реальная отправка;
- отсутствие подходящих друзей;
- friend conversion;
- reward feedback;
- balance utility.

---

## 8. Понимают ли пользователи ценность баланса

**Эмпирически — неизвестно: нет событий и usability-интервью. По информационной полноте текущего экрана — скорее недостаточно.**

### Five-second comprehension test текущего экрана

За пять секунд обычный пользователь, вероятно, увидит:

- это бонусная программа;
- его процент;
- текущий баланс;
- ссылку;
- кнопку пригласить.

Он не успеет или не сможет узнать:

- друг сразу получает 3 бесплатных дня;
- reward появляется только после оплаты;
- hold относится к каждой конкретной оплате;
- точную дату release;
- продлить VPN можно уже от 1 USDT;
- 10 USDT нужны только для вывода;
- бонусы покупают дни, а не обязательно полный тариф;
- mixed checkout отсутствует;
- какой друг зарегистрировался, оплатил или находится в hold.

### Ответы на 12 обязательных вопросов

| Вопрос | Текущий ответ |
|---|---|
| Сколько доступно сейчас? | Да |
| Сколько в ожидании? | Да, агрегатом |
| Откуда деньги? | Нет, только общая статистика |
| Что должен сделать друг? | Частично: «оплата от 1 месяца» спрятана в terms |
| Что сразу получает друг? | Нет на hub |
| Что получаю я? | Да: процент с каждой оплаты |
| Когда доступно? | Да: через 48ч, но без даты |
| Можно потратить на VPN? | Только по названию кнопки |
| Хватит ли на выбранный тариф? | Нет; выбранного тарифа в этом flow вообще нет |
| Можно применить часть и доплатить? | Не объяснено; фактически нет |
| Почему вывод от $10? | Не объяснено |
| Как отправить без ощущения спама? | Есть нейтральный Share-текст, но нет возможности выбрать/отредактировать тон в bot UI |

### Главный смысловой конфликт

Экран одновременно говорит:

- `Баланс: USDT`;
- `Вывод от $10`;
- `Потратить на подписку (-30%)`.

Без явной фразы о пороге 1 USDT пользователь должен самостоятельно вывести правила из трёх разных элементов. Для low-ticket продукта это лишняя когнитивная работа.

---

## 9. Вердикт по частичной оплате от 0,01 USDT

### Короткий ответ

**Не строить mixed checkout сейчас и не обещать использование от 0,01 USDT.**

### Почему

1. **0,01 USDT несовместим с текущей единицей продукта.** Один день после скидки стоит около 0,0441 USDT, дробные дни backend не поддерживает.
2. **Главный разрыв сейчас — discoverability, а не арифметика.** Уже 239 пользователей могут использовать существующий flow от 1 USDT; исторически им воспользовались только 30 пользователей.
3. **Mixed checkout — это новый денежный orchestration:**
   - резерв bonus balance;
   - создание provider order на остаток;
   - срок действия quote и курс;
   - отмена/timeout;
   - refund с разделением источников;
   - chargeback;
   - referral reversal;
   - повторный webhook;
   - reconciliation и support.
4. **Каннибализация не изучена.** Пользователь может тратить бонус перед покупкой и откладывать денежное продление.
5. **Текущий prorated-days flow уже является частичным использованием экономической ценности**, только не частичной оплатой одного заказа.

### Что сравнивать

| Модель | UX | Техника / бухгалтерия | Рекомендация |
|---|---|---|---|
| Текущий prorated spend от 1 USDT | Баланс → целые дни | Уже работает, атомарный ledger/refund | Оставить и ясно объяснить |
| Снизить threshold до 0,05 USDT | Почти любой meaningful balance → ≥1 день | Небольшое изменение, но больше микросписаний | Проверять только после UX baseline |
| От 0,01 USDT | Для 0,01–0,04 результат невозможен | Нужны дробные дни или иной credit unit | Не делать |
| Mixed checkout | Bonus + provider remainder за один тариф | Высокая сложность возвратов/reconciliation | Отложить |
| Offer в обычном checkout | Напомнить о балансе перед оплатой | Если ведёт в текущий spend, checkout не смешивается | Тестировать после referral screen |
| Offer перед expiry | Контекстно продлить бонусами | Уже почти поддержан notifier | Лучший второй timing experiment |

### Условие возврата к mixed checkout

Рассматривать его только если после двух недель нового UX:

- `balance_spend_offer_view → click` высокое;
- пользователи доходят до confirmation;
- но success низкий именно из-за желания оплатить полный тариф с доплатой;
- это подтверждено коротким опросом/причиной отказа, а не предположением.

---

## 10. Три варианта нового referral UX

### Вариант 1. Минимальная корректировка существующего экрана

#### Wireframe

```text
🤝 Бонусы за друзей

Друг получает 3 дня бесплатно.
Вы получаете 20% с каждой его оплаты.

Доступно: 1,63 USDT ≈ 163 ₽
В ожидании: 0,72 USDT

Приглашено: 2 · оплатили: 1

Бонус доступен через 48 часов после оплаты друга.
От 1 USDT — продление VPN.
10 USDT нужны только для вывода.

[🤝 Отправить приглашение]
[🎁 Потратить на подписку]
[💸 Вывести]
[🎤 Я инфлюенсер]
[← Назад]
```

#### Маршруты

- Share → текущий `t.me/share/url`;
- Spend → текущий `ref_spend_sub`;
- Withdraw → текущий `withdraw_request`;
- Influencer → текущий `influencer_info`;
- Back → `start`.

#### Состояния

- 0: все цифры нулевые, spend/withdraw остаются и дают alert;
- 0–1: spend alert с точным порогом;
- 1–10: spend работает, withdraw alert;
- 10+: оба работают;
- special: существующие кнопки добавляются как сейчас.

#### Нажатия

- До Share: 1 внутри screen, 2 от main.
- До spend success: 2 внутри screen.

#### Плюсы

- минимальный diff;
- нулевой риск потерять существующую функцию;
- быстро даёт новый copy baseline.

#### Минусы

- четыре равноправных CTA остаются;
- недоступные действия продолжают выглядеть доступными;
- special/ordinary IA не разделены;
- прогресса друга нет.

#### Аналитика и A/B

50/50 old copy vs new copy. События: screen open, native share open, spend click/success, withdrawal open/request. Главная метрика — share open per screen open; вторичная — spend success per eligible screen open.

---

### Вариант 2. Перестройка иерархии без удаления функций

#### Wireframe обычного пользователя

```text
🤝 Бонусы за друзей

Друг получает 3 дня бесплатно.
Вы получаете 20% с каждой его оплаты.

Доступно: 1,63 USDT ≈ 163 ₽
В ожидании: 0,72 USDT
Приглашено: 2 · оплатили: 1

Бонус появляется после оплаты друга
и становится доступен через 48 часов.

От 1 USDT — продление VPN.
10 USDT нужны только для вывода.

[🤝 Отправить приглашение]
[🎁 Продлить на 36 дней за 1,59 USDT]
[ℹ️ Условия, ссылка и вывод]
[← Назад]
```

Число дней и списание динамические и должны использовать тот же `refSpendCalc`, что confirmation.

#### Secondary screen

```text
ℹ️ Условия и вывод

• Друг получает 3 дня бесплатно.
• Вы получаете 20% с каждой его оплаты.
• Друг закрепляется за вами.
• Бонус доступен через 48 часов после оплаты.
• Продление VPN — от 1 USDT.
• Вывод на USDT-кошелёк — от 10 USDT.

Ваша ссылка:
https://t.me/...

[💸 Вывести USDT]
[🎤 Для авторов и каналов]
[← К бонусам]
```

Для special partner этот secondary screen дополнительно содержит:

- `🎤 Пригласить инфлюенсера`;
- `🔗 Изменить ссылку`;
- `🎁 Запросить год VPN`, если eligible;
- master statistics остаётся доступной.

Для balance ≥10 кнопку `💸 Вывести {balance} USDT` можно поднять на main screen, чтобы не ухудшать путь 18 eligible пользователей.

#### Что стало с функциями

- Share остаётся первым CTA.
- Spend остаётся вторым и становится state-aware.
- Withdraw не удаляется: контекстно на main для eligible, в details для остальных.
- Influencer не удаляется: переносится в details для ordinary; special tools остаются видимыми своему cohort.
- Raw link остаётся в details.
- Stats остаются на main.

#### Состояния

- 0: вторая кнопка `🎁 Как использовать бонусы`;
- 0,01–0,99: `🎁 Продление бонусами — от 1 USDT`;
- 1–9,99: динамическое `Продлить на N дней`;
- 10+: spend + direct withdraw;
- pending >0: отдельная строка с ближайшей датой release;
- special: partner tools не теряют discoverability.

#### Нажатия

- До Share: 1 внутри screen, без ухудшения.
- До spend success: 2 внутри screen, без ухудшения.
- До withdrawal для <10: функция недоступна; explanation за 1.
- До withdrawal для ≥10: 3 действия + wallet input, как сейчас.

#### Риски

- ordinary users реже откроют influencer offer;
- raw-copy link станет на один tap дальше;
- условная клавиатура сложнее тестируется;
- динамический days label должен рассчитываться тем же helper, иначе возникнет расхождение.

#### A/B

- ordinary users: 50% current, 50% hierarchy;
- special partners на первые 7 дней остаются в control;
- затем отдельный cohort test;
- primary: native share open / referral screen open;
- secondary: referred link opens per exposed referrer, spend success per eligible open;
- guardrails: withdrawal requests ≥10, partner contact/custom/invite usage, callback errors.

---

### Вариант 3. Контекстный referral hub с прогрессом друзей и балансом

#### Wireframe

```text
🤝 Бонусы за друзей

Доступно: 1,63 USDT ≈ 163 ₽
В ожидании: 0,72 USDT

Друг получает 3 дня бесплатно.
Вы — 20% после его оплаты.

Последние приглашения:
• @an*** — оплатил · +0,72 в ожидании до 02.08
• @mi*** — активировал 3 дня · ждём оплату

[🤝 Отправить приглашение]
[🎁 Продлить на 36 дней]
[👥 Все приглашения · 2]
[ℹ️ Условия и вывод]
[← Назад]
```

#### Friend-detail states

```text
• зарегистрировался;
• активировал бесплатные 3 дня;
• оплатил — +X USDT в ожидании;
• +X USDT доступно;
• начисление отменено после возврата.
```

#### Backend

Переиспользовать существующий `ReferralInvites`; добавить только отсутствующие для UI поля:

- trial_used;
- earliest unlock_at;
- cancelled/reversed state при необходимости;
- безопасный masked label.

Новая аналитическая платформа не нужна.

#### Нажатия

- Share: 1.
- Spend: 2.
- Friend progress: 1.
- Withdraw: 2 до выбора сети, если в details; direct для ≥10 можно оставить.

#### Плюсы

- закрывает главный trust gap;
- использует уже существующую backend-функцию;
- даёт контекст для repeat referral;
- различает registered, trial, paid, pending, available.

#### Минусы

- больше backend/UI surface;
- нужны privacy rules для имени друга;
- нет доказательства, что progress увеличит Share;
- потенциально провоцирует давление на друга;
- до базовой телеметрии это более крупное изменение, чем требуется.

#### A/B

После baseline и варианта 2:

- control: выбранный hierarchy screen без friend list;
- treatment: две последние masked строки + all invites;
- primary: второй referral link open в течение 30 дней после первого;
- secondary: referral-paid rate;
- guardrails: support/privacy complaints, block/mute rate, no increase in suspicious referral activity.

---

## 11. Выбранный вариант

**Выбран вариант 2 — перестройка иерархии без удаления функций.**

Почему:

1. Исправляет доказанные информационные дефекты: friend benefit, два порога, pending/available, способ траты.
2. Не требует mixed checkout.
3. Не требует сразу строить новый friend-status screen.
4. Не удаляет partner, withdrawal, raw link или special tools.
5. Сохраняет текущие 1 tap до Share и 2 tap до успешного spend внутри hub.
6. Позволяет собрать корректный baseline для решения, нужен ли вариант 3.
7. Использует текущий `refSpendCalc` и существующую `user_events`.

Вариант 3 следует держать как следующий эксперимент, а не как первый релиз.

---

## 12. Точные русские тексты выбранного варианта

### 12.1. Название входа

Оставить:

```text
🤝 Пригласить друга
```

Причина: ясно описывает действие и уже знакомо пользователям. Переименование входа ради стилистики не требуется.

### 12.2. Main referral: balance = 0, hold = 0

```html
🤝 <b>Бонусы за друзей</b>

Друг получает <b>3 дня бесплатно</b>.
Вы получаете <b>{commission_pct}% с каждой его оплаты</b>.

💰 <b>Доступно:</b> 0,00 USDT
⏳ <b>В ожидании:</b> 0,00 USDT
👥 Приглашено: 0 · оплатили: 0

Бонус появится после оплаты друга и станет доступен через {hold_hours} часов.

Бонусами можно продлить VPN от <b>1 USDT</b>.
<b>10 USDT нужны только для вывода</b> на кошелёк.
```

Кнопки:

```text
[🤝 Отправить приглашение]
[🎁 Как использовать бонусы]
[ℹ️ Условия, ссылка и вывод]
[← Назад]
```

### 12.3. Main referral: есть hold, available <1

```html
🤝 <b>Бонусы за друзей</b>

Друг получает <b>3 дня бесплатно</b>.
Вы получаете <b>{commission_pct}% с каждой его оплаты</b>.

💰 <b>Доступно:</b> {balance_usdt} USDT ≈ {balance_rub} ₽
⏳ <b>В ожидании:</b> {hold_usdt} USDT
Ближайшее начисление: {unlock_date}

👥 Приглашено: {registered} · оплатили: {paid}

Бонусами можно продлить VPN от <b>1 USDT</b>.
До этого не хватает <b>{missing_to_one} USDT</b>.
<b>10 USDT нужны только для вывода</b> на кошелёк.
```

Кнопки:

```text
[🤝 Отправить приглашение]
[🎁 Продление бонусами — от 1 USDT]
[ℹ️ Условия, ссылка и вывод]
[← Назад]
```

Alert по второй кнопке:

```text
Для продления нужно минимум 1 USDT.
Сейчас доступно {balance_usdt} USDT — не хватает {missing_to_one} USDT.
Сумма в ожидании станет доступна после даты, указанной на экране.
```

Если hold = 0, последнюю строку заменить:

```text
Новые бонусы появятся после оплаты приглашённого друга.
```

### 12.4. Main referral: available ≥1 и <10

```html
🤝 <b>Бонусы за друзей</b>

Друг получает <b>3 дня бесплатно</b>.
Вы получаете <b>{commission_pct}% с каждой его оплаты</b>.

💰 <b>Доступно:</b> {balance_usdt} USDT ≈ {balance_rub} ₽
⏳ <b>В ожидании:</b> {hold_usdt} USDT
👥 Приглашено: {registered} · оплатили: {paid}

Бонус появляется после оплаты друга и становится доступен через {hold_hours} часов.

Сейчас балансом можно продлить VPN на <b>{days} дней</b>.
<b>10 USDT нужны только для вывода</b> на кошелёк.
```

Кнопки:

```text
[🤝 Отправить приглашение]
[🎁 Продлить на {days} дней за {cost_usdt} USDT]
[ℹ️ Условия, ссылка и вывод]
[← Назад]
```

### 12.5. Main referral: available ≥10

Текст тот же, но последняя часть:

```html
Сейчас балансом можно продлить VPN на <b>{days} дней</b>
или вывести всю доступную сумму на USDT-кошелёк.
```

Кнопки:

```text
[🤝 Отправить приглашение]
[🎁 Продлить на {days} дней за {cost_usdt} USDT]
[💸 Вывести {balance_usdt} USDT]
[ℹ️ Условия и ссылка]
[← Назад]
```

### 12.6. Spend confirmation

```html
🎁 <b>Продлить VPN за бонусы?</b>

Доступно: <b>{balance} USDT</b>
Спишется: <b>{cost} USDT</b>
Подписка продлится на: <b>{days} дней</b>
Останется: <b>{balance_after} USDT</b>

Обычная оплата не потребуется.
```

Кнопки:

```text
[✅ Продлить на {days} дней]
[← К бонусам]
```

### 12.7. Spend success

```html
✅ <b>Подписка продлена</b>

Добавлено: <b>{days} дней</b>
Остаток бонусов: <b>{balance} USDT</b>
```

Кнопки:

```text
[🛡 Моя подписка]
[🤝 К бонусам]
```

### 12.8. Details / conditions

```html
ℹ️ <b>Как работают бонусы</b>

• Друг получает 3 дня бесплатно.
• Вы получаете {commission_pct}% с каждой его оплаты.
• Друг закрепляется за вами.
• После оплаты бонус находится в ожидании {hold_hours} часов.
• От 1 USDT бонусами можно продлить VPN.
• От 10 USDT можно вывести весь доступный баланс на USDT-кошелёк.

🔗 <b>Ваша ссылка:</b>
<code>https://t.me/doodlevpn_bot?start=ref_{ref_code}</code>

Сумма в рублях на предыдущем экране приблизительная.
```

Кнопки ordinary:

```text
[💸 Вывести USDT]
[🎤 Для авторов и каналов]
[← К бонусам]
```

Кнопки special дополнительно:

```text
[🎤 Пригласить инфлюенсера]
[🔗 Изменить ссылку]
[🎁 Запросить год VPN]   — только когда eligible
```

### 12.9. Withdraw below threshold

```text
Для вывода нужно минимум 10 USDT.
Сейчас доступно {balance_usdt} USDT.

Это ограничение относится только к выводу.
Продлить VPN бонусами можно уже от 1 USDT.
```

### 12.10. Share text

Control пока оставить текущим:

```text
Вот хороший VPN, на 3 дня бесплатно дают. Поставь себе
```

Treatment для корректного нового A/B:

```text
Я пользуюсь DoodleVPN. По этой ссылке можно включить VPN на 3 дня бесплатно, без карты:
```

Второй текст менее рекламный и добавляет личный контекст. Его нельзя объявлять лучше до измерения referred link opens и first payments.

### 12.11. Reward pending

```html
⏳ <b>Друг оплатил DoodleVPN</b>

В ожидании: <b>+{reward_usdt} USDT</b>
Бонус станет доступен {unlock_date}.

Доступно сейчас: <b>{available_total} USDT</b>
```

Кнопки:

```text
[🤝 Открыть бонусы]
```

### 12.12. Reward available

```html
✅ <b>Бонус доступен</b>

Начислено: <b>+{reward_usdt} USDT</b>
Доступно всего: <b>{available_total} USDT</b>
```

Если total ≥1:

```text
[🎁 Продлить VPN за бонусы]
[🤝 Открыть бонусы]
```

Если total <1:

```text
[🤝 Открыть бонусы]
```

---

## 13. Реестр перемещения функций

| Функция | Было | Стало | Почему | Риск | Обратимость |
|---|---|---|---|---|---|
| Native Share | Первая кнопка | Первая кнопка | Главный CTA | Минимальный | Старый keyboard |
| Spend balance | Одинаковая кнопка во всех states | Динамическая вторая кнопка | Показывает реальный результат | Ошибка расчёта label | Один feature flag |
| Withdraw ≥10 | Одинаковая кнопка | Direct main CTA для eligible | Не ухудшать payout | Дополнительный CTA для 18 users | Cohort condition убрать |
| Withdraw <10 | Одинаковая кнопка + alert | Details + точное объяснение | Не обещать недоступное действие | Снижение узнаваемости payout | Вернуть на main |
| Influencer offer ordinary | Равный CTA | Details → «Для авторов и каналов» | Не конкурирует с friend Share | Меньше partner leads | Измерять contact clicks; вернуть |
| Invite influencer special | Main после общих кнопок | Partner tools, видимые special cohort | Сохраняет операционный доступ | Можно ухудшить habitual path | Special users оставить в control сначала |
| Custom referral code | Main special | Partner tools | Группировка операций | Discoverability | Старый special keyboard |
| Annual VPN request | Main/notification по eligibility | Partner tools + eligibility | Cohort-specific benefit | Partner support | Не менять первые 7 дней |
| Raw referral link | В основном тексте | Details | Сокращает decision load, функция сохранена | Ручное copy на 1 tap дальше | A/B + вернуть строку |
| Aggregate stats | Expandable blockquote | Три строки на main | Быстрее считывается state | Меньше historical detail | Total earned оставить в details |
| Master stats | Большой block на main | Не менять в первой итерации special cohort | Высокая операционная ценность | Перегрузка special screen остаётся | Нулевой |

---

## 14. Функции, которые нельзя трогать без отдельного финансового/операционного ревью

1. Permanent first-referrer attribution.
2. Self-referral guard и custom-ref validation.
3. Risk/abuse gates.
4. Проверка канонического paid/completed перед начислением.
5. 48-часовой hold, пока владелец не изменит экономическую политику.
6. Idempotency referral hold по payment/referrer.
7. Refund reversal и защита от отрицательного баланса.
8. Атомарный `StartRefSpend → EnsureUser → Complete/Refund`.
9. Повторный расчёт live balance на confirmation.
10. Целые дни и единый `refSpendCalc`, пока не выбран другой credit unit.
11. Порог вывода и вывод всего баланса без согласования с finance/ops.
12. Валидация BEP20/TON-кошельков.
13. Админское approve/reject и возврат зарезервированного balance.
14. Special partner, master-influencer и custom-link функции.
15. Ручная проверка annual influencer subscription.
16. Daily limit приглашения инфлюенсеров.
17. Required-channel gate.
18. Legal screens.
19. Payment provider fulfillment, webhook/poller и provider status normalization.
20. Gift subscription.
21. Device/LTE add-ons до отдельного usage/finance анализа.
22. Тексты/кнопки support recovery, связанные с работающими операционными процессами.

---

## 15. Минимальный instrumentation plan

Новая analytics platform не нужна. Достаточно существующей `user_events`:

```text
tg_id
event_name
event_ts
variant_id
source_payload / entry_kind
referrer_id
metadata_json
```

### 15.1. Минимальные события

| Event | Когда писать | Контекст |
|---|---|---|
| `main_menu_view` | После успешной отправки/замены main menu | user_state, referral_button_visible |
| `referral_screen_open` | После рендера referral main | source, balance_bucket, hold_bucket, ref_count_bucket, partner_role, variant |
| `referral_native_share_open` | Перед redirect в `t.me/share/url` | source, share_copy_variant, ref_code_kind |
| `referral_link_open` | `/start ref_*` | referrer_id, ref_code_kind, first_touch |
| `referred_registration` | Первый successful user insert с referrer | referrer_id |
| `referred_trial_start` | Trial activation у referred user | referrer_id, days |
| `referred_first_payment` | Первая paid запись referred user | referrer_id, plan, provider |
| `reward_pending` | Успешно создан новый hold | referrer_id, reward_amount_bucket, unlock_hours |
| `reward_available` | Hold реально переведён в available | referrer_id, reward_amount_bucket, available_total_bucket |
| `balance_spend_offer_view` | State-aware spend CTA показан | balance_bucket, days_bucket, source |
| `balance_spend_click` | Открыт confirmation | balance_bucket, days, cost |
| `balance_spend_success` | CompleteRefSpend successful | days, cost_bucket, source |
| `withdrawal_open` | Открыт network picker либо threshold explanation | eligible, balance_bucket |
| `withdrawal_request` | Request создан | network, amount_bucket |
| `withdrawal_success` | Admin approve completed | network, amount_bucket |
| `partner_tools_open` | Ordinary/special открыл tools | partner_role |
| `partner_contact_open` | Через tracked redirect открыт contact | partner_role |

### 15.2. Не называть событием

- Не писать `referral_share_sent`: Telegram не сообщает, отправил ли человек сообщение.
- Не считать `native_share_open` фактической отправкой.
- Не считать `/start ref_*` уникальной регистрацией, если user уже существовал.
- Не считать paid самого реферера referral conversion.

### 15.3. Как измерить URL-кнопку

URL-кнопка не вызывает bot callback. Использовать уже знакомый проекту паттерн tracked redirect:

```text
https://public-host/r/ref-share/<opaque-token>
  → log referral_native_share_open
  → 302 на https://t.me/share/url?...
```

Токен не должен раскрывать raw TG ID. Новая система событий и сторонний SDK не нужны.

### 15.4. Минимальная sessionization

Не обязательно сразу вводить полноценные sessions. Для referral experiment достаточно:

- stable `variant_id`;
- `source_payload` = `main_menu`, `guide_done`, `post_pay_nudge`, `reward_available`, `expiry`;
- server-generated `flow_id` на один referral screen render, переданный в redirect token.

---

## 16. Эксперименты: control и holdout

### 16.1. Сначала baseline

Первые 3–7 дней добавить только события и tracked Share redirect, не менять тексты и порядок кнопок. Это создаст:

- referral open rate;
- native Share open rate;
- ref-link open per exposed referrer;
- текущий spend/withdraw funnel.

### 16.2. UI A/B

Аудитория: ordinary users. Special partners первые 7 дней исключены.

- Control 50%: текущий referral screen.
- Treatment 50%: выбранный вариант 2.
- Assignment: стабильный hash TG ID + experiment name, сохранённый в существующем `experiment_states` или вычисленный детерминированно.
- Минимальное окно: 14 дней.
- Анализ: по exposed referrers, а не по всем users.

Primary metrics:

1. `referral_native_share_open / referral_screen_open`;
2. `referral_link_open / unique exposed referrer`;
3. `balance_spend_success / eligible referral_screen_open`.

Secondary:

- referred registration;
- referred trial;
- referred first payment;
- повторный Share в 30 дней;
- второй referred link open после первого.

Guardrails:

- callback error rate;
- withdrawal request rate среди balance ≥10;
- partner contact rate;
- regular plan purchase/renewal;
- support complaints.

### 16.3. Notification experiment

Отдельно от UI:

- 45% control: без новых reward notifications;
- 45% treatment: pending + available;
- 10% stable holdout: без notification, но все события пишутся.

Primary:

- referral screen reopen в 7 дней после reward;
- spend success в 14 дней после available;
- повторный native Share open.

Guardrails:

- bot blocks/mutes;
- support complaints;
- notification send failure;
- duplicate messages.

### 16.4. Share copy A/B

Сравнивать не оплату самого реферера, а:

```text
referral_native_share_open
→ unique referral_link_open
→ referred_registration
→ referred_first_payment
```

Поскольку фактическая отправка неизвестна, denominator для downstream результата — exposed referrers или native share opens, с одинаковым определением в обеих группах.

### 16.5. Payment rescue

Не смешивать с referral UI experiment. После проверки eligibility:

- 90% enabled;
- 10% существующий stable holdout;
- анализ по provider order;
- primary: paid within 24/48h;
- guardrails: duplicate notification, complaint/block, refunded rate.

---

## 17. План внедрения на 3 / 7 / 14 / 30 дней

### День 0–3: измеримость и исправление ложных обещаний

1. Добавить referral events в существующую таблицу.
2. Поставить tracked redirect перед native Share.
3. Добавить `reward_pending` и `reward_available` event без пользовательских сообщений.
4. Удалить из notifier-комментария ложное правило «ниже 2,10 нельзя купить».
5. Подготовить feature flag старого/нового referral keyboard.
6. Не менять финансы, threshold и checkout.

Acceptance:

- события создаются один раз;
- redirect сохраняет правильную Share ссылку;
- ref deep-link attribution не ломается;
- raw IDs не попадают в URL/логи.

### День 4–7: baseline и copy correctness

1. Собрать baseline current screen.
2. Проверить доли state buckets.
3. Включить корректные referral-balance expiry тексты на небольшой cohort.
4. Исправить alert <1 с точным threshold/shortfall.
5. Special partners оставить в старом UI.

Решение дня 7:

- если events/redirect неполны — не запускать hierarchy A/B;
- если baseline стабилен — начать 50/50.

### День 8–14: A/B выбранного referral screen

1. Ordinary users 50/50.
2. Pending/available notifications — отдельный 45/45/10 experiment.
3. Ежедневно следить только за kill criteria, не выбирать winner раньше.
4. Не менять Share copy одновременно с IA.

Решение дня 14:

- treatment продолжать, если primary metric направленно растёт и guardrails целы;
- не объявлять winner на малом числе downstream payments;
- зафиксировать minimum sample для следующего окна.

### День 15–30: подтверждение и следующий слой

1. Довести UI experiment до двух полных недель.
2. Отдельно запустить Share copy A/B.
3. Если hierarchy работает — подключить special cohort с сохранением direct tools.
4. Только затем тестировать вариант 3 с двумя последними friend states.
5. Оценить payment rescue отдельно.
6. Не начинать mixed checkout до evidence gate.

Решение дня 30:

- закрепить variant либо откатить;
- определить, нужен ли friend-progress hub;
- определить, имеет ли смысл threshold 0,05–1,00;
- собрать отдельный brief на mixed checkout только при доказанном спросе.

---

## 18. Kill-критерии и rollback

### Немедленный kill

Откатить treatment полностью при любом из условий:

1. duplicate referral debit/credit или расхождение баланса — хотя бы один подтверждённый случай;
2. raw TG ID или wallet попал в публичный tracked URL/analytics metadata;
3. broken ref attribution >0,5% проверенных deep-links;
4. callback/error rate treatment >1%;
5. Share redirect не открывает корректный Telegram composer;
6. reward notification отправлена по cancelled/refunded hold;
7. withdrawal для eligible пользователя стал недоступен.

### Статистические/продуктовые stop rules

После минимального sample:

- native Share open rate падает более чем на 10% relative;
- referral link opens на exposed referrer падают более чем на 10%;
- withdrawal requests среди eligible ≥10 падают более чем на 20% без компенсирующего spend;
- partner contact/custom/invite usage падает более чем на 20% в special cohort;
- regular renewal conversion падает более чем на 5%;
- bot block/mute после reward notification растёт более чем на 20% relative;
- support обращения «не могу найти вывод/ссылку» заметно растут.

### Rollback

1. Server-side feature flag возвращает старый `ReferralKB` и текущий copy.
2. Tracked redirect можно оставить: он не меняет продуктовую семантику.
3. События оставить: они read-only относительно продукта.
4. Notification spec отключается отдельным flag; уже отправленные сообщения не удалять.
5. Financial handlers, ledger и thresholds вообще не участвуют в rollback.

---

## 19. Точный список первых изменений в боте

Это список к реализации **только после одобрения владельцем**.

### Первый PR: только измеримость

1. `handleReferral`: записывать `referral_screen_open` со state buckets.
2. Заменить прямой Share URL на opaque tracked redirect.
3. Redirect: записывать `referral_native_share_open`, затем 302 в Telegram.
4. `/start ref_*`: дополнительно писать `referral_link_open`; на первом user insert — `referred_registration`.
5. Trial/payment lifecycle: писать `referred_trial_start`, `referred_first_payment`.
6. `CreditReferrer`: после реально вставленного hold писать `reward_pending`.
7. `ReleaseEligibleReferralHolds`: возвращать не только referrer IDs, а минимальные notification facts либо читать их после commit; писать `reward_available`.
8. Spend/withdraw: добавить offer/click/success events.
9. Не добавлять новую БД или dependency.

### Второй PR: correctness без перестройки IA

1. Alert <1: текущая сумма, минимум, shortfall.
2. Referral-balance expiry notifier:
   - eligibility от 1 USDT;
   - убрать «подарочные рубли»;
   - явно сказать, что речь о продлении на дни;
   - переименовать internal kind/comment так, чтобы не создавать впечатление expiry balance.
3. Добавить unit tests на exact state buckets и dynamic spend label.

### Третий PR: выбранный вариант 2 под feature flag

1. Новый copy main referral.
2. State-aware keyboard.
3. Details screen.
4. Ordinary vs special partner keyboards.
5. Direct withdraw для balance ≥10.
6. Dynamic spend label из того же `refSpendCalc`.
7. Stable 50/50 assignment.
8. Не трогать financial handlers.

### Четвёртый PR: reward notifications experiment

1. Pending notification после commit.
2. Available notification после successful release commit.
3. Stable 10% holdout.
4. Дедупликация по hold ID / notification key.
5. Отдельный kill switch.

### Не делать в первых изменениях

- mixed checkout;
- threshold 0,01;
- новый game layer;
- удаление influencer/withdraw/raw link;
- добавление referral на first-time menu;
- friend-progress list;
- новый analytics service;
- одновременный редизайн payment checkout.

---

## Финальное готовое решение

| Вопрос владельца | Решение |
|---|---|
| Какой экран менять первым? | `🤝 Бонусы за друзей` |
| Что оставить? | Share, balance, hold, stats, spend, withdraw, partner tools, raw link, back |
| Что сгруппировать? | Terms, raw link, payout для неeligible, ordinary influencer offer |
| Что показывать контекстно? | Spend result по balance; direct withdraw при ≥10; special partner tools только special cohort |
| Главный CTA | `🤝 Отправить приглашение` |
| Формулировка оффера | `Друг получает 3 дня бесплатно. Вы получаете 20% с каждой его оплаты.` |
| Как показать баланс? | Отдельно `Доступно` и `В ожидании`, плюс ближайшая дата release |
| Как объяснить $10? | `10 USDT нужны только для вывода. Продлить VPN можно от 1 USDT.` |
| Где предлагать spend? | На referral main и перед expiry; позже тестировать напоминание в обычном checkout |
| Нужен ли mixed checkout? | Нет, пока не доказан спрос после исправления discovery |
| Какие события добавить? | Open/share/link/referred lifecycle/reward/spend/withdraw + variant/source |
| Что подтвердит улучшение за две недели? | Рост Share open и ref-link opens на exposed referrer; рост spend success у eligible; без падения withdrawal, partner и renewal guardrails |

### Критерий успеха через две недели

Не использовать заранее придуманный абсолютный KPI без baseline. Treatment можно считать перспективным, если одновременно:

1. `referral_native_share_open / referral_screen_open` вырос минимум на 15% relative;
2. `referral_link_open / exposed referrer` вырос минимум на 10% relative;
3. `balance_spend_success / eligible referral_screen_open` вырос минимум на 20% relative;
4. withdrawal и partner guardrails не ухудшились за kill thresholds;
5. нет финансовых, privacy или callback инцидентов.

Если Share растёт, а ref-link opens нет, проблема в Share copy/реальной отправке. Если link opens растут, но registrations/trial нет, проблема после deep-link. Если balance offer clicks растут, но success нет, только тогда следует исследовать mixed checkout или другой minimum.

---

## Приложение A. Источники доказательств

Основные production-файлы:

- `bot-go/internal/app/app.go` — маршрутизация, start, trial, payment, jobs;
- `bot-go/internal/ui/keyboards.go` — пользовательские клавиатуры;
- `bot-go/internal/app/handlers_referral.go` — referral, partner, withdrawal;
- `bot-go/internal/app/handlers_ref_spend_sub.go` — продление бонусами;
- `bot-go/internal/app/handlers_guide.go` — setup и contextual Share;
- `bot-go/internal/app/notifiers_expiry.go` — expiry/winback/ref-balance messages;
- `bot-go/internal/app/notifiers_payment.go` — выключенный payment rescue;
- `bot-go/internal/app/notifiers_onboarding.go` — pretrial/post-pay invite timing;
- `bot-go/internal/store/store.go` — schema, attribution, referral holds, ledger, events;
- `bot-go/internal/store/admin.go` — историческая Share A/B aggregation;
- `bot-go/internal/i18n/texts_ru.go` — текущие русские тексты.

Проверенные production flags:

```text
SUBSVC_ENABLED=true
PROFILE_MENU_PUBLIC_ENABLED=true
MOBILE_APP_CODE_PUBLIC_ENABLED=true
WEB_LTE_ENABLED=true
ANTI_TRAFFIC_NOTIFIERS_ENABLED=true
LIFECYCLE_RESCUE_NOTIFIERS_ENABLED=false (default)
PAYMENT_RESCUE_NOTIFIERS_ENABLED=false (default)
REFERRAL_OUTREACH_NOTIFIERS_ENABLED=false (default)
```

## Приложение B. Что потребуется для качественной проверки после релиза

После instrumentation, но до следующего крупного IA-решения:

1. 5–8 коротких moderated tests на current/treatment referral screen;
2. один вопрос после неуспешного spend click: что ожидал пользователь;
3. классификация support tickets по referral/payment/withdraw;
4. cohort comparison ordinary vs special partner;
5. 14 дней A/B без одновременной смены Share copy;
6. 30 дней для referred first payment, потому что 14 дней может быть мало для downstream outcome.
