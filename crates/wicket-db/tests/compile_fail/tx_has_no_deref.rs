fn needs_deref<T: std::ops::Deref>() {}
fn needs_deref_mut<T: std::ops::DerefMut>() {}

fn main() {
    needs_deref::<wicket_db::Tx<'_>>();
    needs_deref_mut::<wicket_db::Tx<'_>>();
}
