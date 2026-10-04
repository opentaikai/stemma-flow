use stemma_flow::db;

fn main() -> rusqlite::Result<()> {
    let conn = db::open_connection("stemma-flow.db")?;
    db::init_db(&conn)?;
    println!("Database ready at stemma-flow.db");
    Ok(())
}
