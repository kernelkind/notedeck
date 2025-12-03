use enostr::{Pubkey, RelayPool};
use nostrdb::{Filter, FilterBuilder};
use uuid::Uuid;

pub fn remote_sub(pool: &mut RelayPool, filters: Vec<Filter>) -> String {
    let subid = Uuid::new_v4().to_string();

    pool.subscribe(subid.clone(), filters);
    subid
}

static GIFT_WRAP_KIND: u64 = 1059;

pub fn giftwrap_filter(receiver: &Pubkey) -> Filter {
    FilterBuilder::new()
        .pubkey([receiver.bytes()])
        .kinds([GIFT_WRAP_KIND])
        .build()
}
