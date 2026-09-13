use wicket_core::{PostingHandle, PostingId};

fn needs_id(_id: PostingId) {}
fn needs_handle(_h: PostingHandle) {}

fn main() {
    needs_id(PostingHandle(0));
    needs_handle(PostingId(0));
}
