//! 数据层行为测试：真实 SQLite 文件上的完整回路。
//!
//! 覆盖计划验证要点：demo 形态数据入库 → 卡片墙查询 → 中文搜索（FTS 与 LIKE 双路径）→
//! 标签/星标/补写想法 → 重开连接持久化 → 复习评分 Again 的 10 分钟 due。

use mudflat_knowledge_lib::db::{self, CardFilter, NewBook, UpsertCard};
use mudflat_knowledge_lib::srs::{self, Rating};

fn tmp_db(name: &str) -> (std::path::PathBuf, rusqlite::Connection) {
    let dir = std::env::temp_dir().join(format!("mudflat-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 先开一次确保 schema 建好，再交由调用方重开使用
    drop(db::open_db(&dir).unwrap());
    (dir.clone(), db::open_db(&dir).unwrap())
}

fn insert_demo_book(conn: &rusqlite::Connection, wid: &str, title: &str, texts: &[&str]) -> i64 {
    db::upsert_book(
        conn,
        &NewBook {
            weread_book_id: wid.into(),
            title: title.into(),
            author: "测试作者".into(),
            cover: String::new(),
            reading_progress: 50,
            note_count: texts.len() as i64,
            review_count: 0,
            wr_sort: None,
            category: String::new(),
        },
    )
    .unwrap();
    let book_row = db::find_book_row(conn, wid).unwrap().unwrap();
    for (i, t) in texts.iter().enumerate() {
        db::upsert_card(
            conn,
            &UpsertCard {
                kind: "highlight",
                book_row_id: book_row,
                remote_id: &format!("{wid}-bm-{i}"),
                chapter_uid: Some(1),
                chapter_title: Some("第一章"),
                text: t,
                abstract_text: None,
                range_str: Some("100-200"),
                color_style: (i as i64 % 5) + 1,
                created_at: 1_700_000_000 + i as i64 * 86400,
            },
            // 划线时间在前、同步时刻在其后：与真实语义一致（created_at 只被防未来的钳制）
            1_700_000_000 + i as i64 * 86400 + 60,
        )
        .unwrap();
    }
    book_row
}

#[test]
fn demo_flow_query_search_persist_review() {
    let (dir, conn) = tmp_db("main");

    // ---- demo 书籍 × 8 划线，第 4 条含「记忆」----
    insert_demo_book(
        &conn,
        "b-1",
        "记忆的书",
        &[
            "大脑如何形成记忆是神经科学的核心问题。",
            "工作记忆容量有限。",
            "无关键词句子 alpha。",
            "另一个句子 beta gamma。",
            "遗忘曲线与复习节奏。",
            "无关键词句子 delta。",
            "间隔重复对抗遗忘。",
            "第八条普通划线。",
        ],
    );
    let b2 = insert_demo_book(&conn, "b-2", "第二本书", &["别把「记忆」写在这里也行。"]);
    assert!(db::find_book_row(&conn, "b-2").unwrap().is_some());
    let _ = b2;

    // ---- 卡片墙：24 不要求（此处 9 条），但过滤正确 ----
    let all = db::query_cards(&conn, &CardFilter::default(), 500, 0).unwrap();
    assert_eq!(all.len(), 9);
    assert_eq!(db::count_cards(&conn, &CardFilter::default()).unwrap(), 9);
    assert!(all.iter().all(|c| !c.deleted));

    let by_book = db::query_cards(
        &conn,
        &CardFilter { book_id: db::find_book_row(&conn, "b-1").unwrap(), ..CardFilter::default() },
        500,
        0,
    )
    .unwrap();
    assert_eq!(by_book.len(), 8);
    assert_eq!(
        db::count_cards(
            &conn,
            &CardFilter { book_id: db::find_book_row(&conn, "b-1").unwrap(), ..CardFilter::default() },
        )
        .unwrap(),
        8
    );

    // 回归锁定：book_id 必须作为绑定参数而非字面量嵌入 SQL。
    // b-2 的行 id 为 2 —— 若实现退化为 c.book_id=1，这里会错误返回 b-1 的卡片。
    let by_book2 = db::query_cards(
        &conn,
        &CardFilter { book_id: db::find_book_row(&conn, "b-2").unwrap(), ..CardFilter::default() },
        500,
        0,
    )
    .unwrap();
    assert_eq!(by_book2.len(), 1);
    assert_eq!(by_book2[0].text, "别把「记忆」写在这里也行。");

    // ---- 搜索：≥3 字中文走 FTS；<3 字走 LIKE ----
    let fts = db::search_cards(&conn, "记忆", &CardFilter::default(), 100).unwrap();
    // 「记忆」是 2 字 —— 应走 LIKE 路径且命中
    assert!(!fts.is_empty(), "2 字短词必须能 LIKE 命中");
    let fts4 = db::search_cards(&conn, "工作记忆", &CardFilter::default(), 100).unwrap();
    assert_eq!(fts4.len(), 1, "≥3 字子串应精确命中一条");
    assert_eq!(fts4[0].text, "工作记忆容量有限。");

    // ---- 标签 / 星标 / 补写想法 ----
    db::add_tag_to_card(&conn, fts4[0].id, "心理学").unwrap();
    db::set_starred(&conn, fts4[0].id, true).unwrap();
    db::update_card_note(&conn, fts4[0].id, "这条值得反复读", 1_700_001_000).unwrap();

    let starred = db::query_cards(
        &conn,
        &CardFilter { starred_only: true, ..CardFilter::default() },
        500,
        0,
    )
    .unwrap();
    assert_eq!(starred.len(), 1);
    assert_eq!(starred[0].note, "这条值得反复读");
    assert_eq!(starred[0].tags, vec!["心理学"]);

    let tag_rows = db::list_tags(&conn).unwrap();
    assert_eq!(tag_rows.len(), 1);

    // ---- 自建卡 + 硬删 ----
    let self_id = db::create_self_card(&conn, "我的独立卡片", 1_700_002_000).unwrap();
    assert_eq!(db::query_cards(&conn, &CardFilter::default(), 500, 0).unwrap().len(), 10);
    db::hard_delete_card(&conn, self_id).unwrap();
    assert_eq!(db::query_cards(&conn, &CardFilter::default(), 500, 0).unwrap().len(), 9);

    // ---- 复习队列与评分：Again → 10 分钟后到期 ----
    let now = 1_800_000_000i64;
    let due_before = db::due_cards(&conn, now, 30, None).unwrap();
    assert_eq!(due_before.len(), 9); // 全部新卡默认当期进入队列（自建卡已硬删）

    let st = db::load_review_state(&conn, due_before[0].id).unwrap().unwrap_or_default();
    let next = srs::schedule(&st, Rating::Again, now);
    db::save_review_state(&conn, due_before[0].id, &next).unwrap();
    let reloaded = db::load_review_state(&conn, due_before[0].id).unwrap().unwrap();
    assert_eq!(reloaded.due_at - reloaded.interval_days as i64 * 86400, now + 600);

    let due_after = db::due_cards(&conn, now, 30, None).unwrap();
    assert_eq!(due_after.len(), 8, "Again 的卡 10 分钟内不再出现在当前队列");

    // ---- reconcile：第二次同步少了一条时按远端删除软删、用户编辑保留 ----
    let keep = vec!["b-1-bm-0".to_string(), "b-1-bm-1".to_string()];
    let removed = db::reconcile_cards(&conn, db::find_book_row(&conn, "b-1").unwrap().unwrap(), &keep)
        .unwrap();
    assert_eq!(removed, 6);
    let after_reconcile = db::query_cards(&conn, &CardFilter::default(), 500, 0).unwrap();
    assert_eq!(after_reconcile.len(), 3); // b-1 剩 2 条 + b-2 的 1 条
    drop(conn);

    // ---- 重开连接：SQLite 持久化证明 ----
    let conn2 = db::open_db(&dir).unwrap();
    let persisted = db::query_cards(&conn2, &CardFilter::default(), 500, 0).unwrap();
    assert_eq!(persisted.len(), 3);
    let the_card = persisted.iter().find(|c| c.text == "工作记忆容量有限。").expect("edited card persists");
    assert!(the_card.starred);
    assert_eq!(the_card.note, "这条值得反复读");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn user_hidden_card_is_not_revived_by_upsert_and_keeps_edits() {
    let (_dir, conn) = tmp_db("tombstone");
    insert_demo_book(&conn, "b-x", "书X", &["原始划线文本"]);

    // 用户改了 note 并执行「隐藏」（用户墓碑）
    let card = &db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap()[0];
    db::update_card_note(&conn, card.id, "用户批注", 111).unwrap();
    db::hide_card_from_user(&conn, card.id).unwrap();
    assert_eq!(db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap().len(), 0);

    // 下次同步同 remote_id 再次出现：不得复活（R4），用户编辑保留在库中
    let book_row = db::find_book_row(&conn, "b-x").unwrap().unwrap();
    let (id, inserted) = db::upsert_card(
        &conn,
        &UpsertCard {
            kind: "highlight",
            book_row_id: book_row,
            remote_id: "b-x-bm-0",
            chapter_uid: Some(1),
            chapter_title: Some("第一章"),
            text: "原始划线文本（可能被服务端更新）",
            abstract_text: None,
            range_str: Some("100-200"),
            color_style: 2,
            created_at: card.created_at,
        },
        222,
    )
    .unwrap();
    assert_eq!(id, card.id);
    assert!(!inserted);
    assert_eq!(db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap().len(), 0,
        "用户隐藏墓碑生效：upsert 不复活");
    let hidden: (i64, i64, String) = conn
        .query_row(
            "SELECT deleted, hidden_by_user, note FROM cards WHERE id=?1",
            [card.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(hidden, (1, 1, "用户批注".into()), "墓碑与用户批注都在");
}

#[test]
fn reconcile_deleted_card_is_revived_when_remote_readds() {
    let (_dir, conn) = tmp_db("reconcile-revive");
    insert_demo_book(&conn, "b-r", "书R", &["远端又加回来了"]);
    let book_row = db::find_book_row(&conn, "b-r").unwrap().unwrap();

    // 远端删除（reconcile）：软删 + 清墓碑，与用户隐藏分开
    db::reconcile_cards(&conn, book_row, &[]).unwrap();
    assert_eq!(db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap().len(), 0);

    // 远端重新出现：应复活（这不是用户决策的删除）
    db::upsert_card(
        &conn,
        &UpsertCard {
            kind: "highlight",
            book_row_id: book_row,
            remote_id: "b-r-bm-0",
            chapter_uid: Some(1),
            chapter_title: Some("第一章"),
            text: "远端又加回来了",
            abstract_text: None,
            range_str: Some("1-2"),
            color_style: 1,
            created_at: 1_700_000_000,
        },
        333,
    )
    .unwrap();
    let revived = db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap();
    assert_eq!(revived.len(), 1, "远端删除的卡在远端恢复后重新可见");
    assert!(!revived[0].deleted);
}

#[test]
fn excluded_restore_puts_card_due_immediately() {
    let (_dir, conn) = tmp_db("excluded-restore");
    insert_demo_book(&conn, "b-e2", "书E2", &["第一条", "第二条"]);
    let cards = db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap();
    let now = 1_900_000_000i64;

    db::set_excluded_from_review(&conn, cards[0].id, true, now).unwrap();
    assert_eq!(db::due_count(&conn, now, None).unwrap(), 1, "移出后到期数减 1");
    // 排除状态不影响墙与搜索
    assert_eq!(db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap().len(), 2);
    assert_eq!(db::search_cards(&conn, "第一条", &CardFilter::default(), 10).unwrap().len(), 1);

    // 恢复：立即回到待回顾状态
    db::set_excluded_from_review(&conn, cards[0].id, false, now + 100).unwrap();
    let st = db::load_review_state(&conn, cards[0].id).unwrap().unwrap();
    assert_eq!(st.due_at, now + 100, "恢复后 due_at=now，立即进入待回顾");
    assert_eq!(db::due_count(&conn, now + 100, None).unwrap(), 2);
}

#[test]
fn upsert_and_reconcile_preserve_excluded_state() {
    let (_dir, conn) = tmp_db("excluded-preserve");
    insert_demo_book(&conn, "b-p", "书P", &["保持排除"]);
    let card = &db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap()[0];
    db::set_excluded_from_review(&conn, card.id, true, 1_700_000_000).unwrap();

    let book_row = db::find_book_row(&conn, "b-p").unwrap().unwrap();
    db::upsert_card(
        &conn,
        &UpsertCard {
            kind: "highlight",
            book_row_id: book_row,
            remote_id: "b-p-bm-0",
            chapter_uid: Some(1),
            chapter_title: Some("第一章"),
            text: "保持排除（服务端更新）",
            abstract_text: None,
            range_str: Some("1-2"),
            color_style: 1,
            created_at: card.created_at,
        },
        1_700_000_500,
    )
    .unwrap();
    db::reconcile_cards(&conn, book_row, &["b-p-bm-0".to_string()]).unwrap();
    let after = db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap();
    assert_eq!(after.len(), 1);
    assert!(after[0].excluded_from_review, "upsert 与 reconcile 均不重置排除状态");
}

#[test]
fn upsert_keeps_remote_highlight_time_and_self_heals() {
    // created_at 必须是远端 createTime（真实划线时间），不是同步时刻；
    // 旧库把 created_at 固化成同步时刻的数据，靠下次同步用远端值覆盖自愈。
    let (_dir, conn) = tmp_db("created-at");
    let book_row = insert_demo_book(&conn, "b-c", "时间书", &["时间锚点"]);
    // 模拟旧库污染：created_at 已被钳成同步时刻
    conn.execute("UPDATE cards SET created_at=1_700_000_060 WHERE remote_id='b-c-bm-0'", [])
        .unwrap();

    let highlight_time = 1_600_000_000i64; // 2020-09，远早于同步时刻
    let (id, inserted) = db::upsert_card(
        &conn,
        &UpsertCard {
            kind: "highlight",
            book_row_id: book_row,
            remote_id: "b-c-bm-0",
            chapter_uid: Some(1),
            chapter_title: Some("第一章"),
            text: "时间锚点（远端原文）",
            abstract_text: None,
            range_str: Some("1-2"),
            color_style: 1,
            created_at: highlight_time,
        },
        1_700_000_100,
    )
    .unwrap();
    assert!(!inserted);
    let card = &db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap()[0];
    assert_eq!(card.id, id);
    assert_eq!(card.created_at, highlight_time, "created_at 修回真实划线时间，而非同步时间");

    // 防时钟偏差：远端时间跑到本地未来时钳到同步时刻
    db::upsert_card(
        &conn,
        &UpsertCard {
            kind: "highlight",
            book_row_id: book_row,
            remote_id: "b-c-bm-0",
            chapter_uid: Some(1),
            chapter_title: Some("第一章"),
            text: "时间锚点（远端原文）",
            abstract_text: None,
            range_str: Some("1-2"),
            color_style: 1,
            created_at: 1_900_000_000,
        },
        1_700_000_200,
    )
    .unwrap();
    let card = &db::query_cards(&conn, &CardFilter::default(), 10, 0).unwrap()[0];
    assert_eq!(card.created_at, 1_700_000_200, "未来时间戳钳到同步时刻");
}

#[test]
fn fts_query_special_chars_do_not_error() {
    // PRD 11.8：FTS 语法字符必须被当普通文本，不得让搜索报 500 式错误
    let (_dir, conn) = tmp_db("fts-escape");
    insert_demo_book(&conn, "b-f", "书F", &["带引号的\"文本\"一", "普通句子 alpha", "带连字符的-词组"]);
    for q in ["\"文本", "alpha- beta", "OR NOT", "普通句子", "a-b-c"] {
        let r = db::search_cards(&conn, q, &CardFilter::default(), 50);
        assert!(r.is_ok(), "查询 {q:?} 不应报错：{:?}", r.err());
    }
    // 转义后仍能命中内容本身
    let hit = db::search_cards(&conn, "普通句子", &CardFilter::default(), 50).unwrap();
    assert_eq!(hit.len(), 1);
    assert_eq!(hit[0].text, "普通句子 alpha");
}

#[test]
fn sync_meta_roundtrip_on_fresh_db() {
    let (_dir, conn) = tmp_db("sync_meta");
    assert_eq!(db::get_sync_meta(&conn, "last_full_sync").unwrap(), None);
    db::set_sync_meta(&conn, "last_full_sync", "1700000000").unwrap();
    db::set_sync_meta(&conn, "last_full_sync", "1700000500").unwrap();
    assert_eq!(
        db::get_sync_meta(&conn, "last_full_sync").unwrap().as_deref(),
        Some("1700000500"),
        "upsert 应覆盖旧值"
    );
}
