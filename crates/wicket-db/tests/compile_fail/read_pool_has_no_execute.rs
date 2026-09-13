fn assert_no_execute(pool: &wicket_db::ReadPool) {
    let _ = pool.execute("SELECT 1");
}

fn main() {
    let _ = assert_no_execute;
}
