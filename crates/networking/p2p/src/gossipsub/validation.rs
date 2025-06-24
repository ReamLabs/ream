use anyhow::{anyhow, ensure};
use ream_consensus::{electra::beacon_block::SignedBeaconBlock, misc::compute_start_slot_at_epoch};
use ream_storage::{db::ReamDB, tables::Field};
use ssz::Decode;

pub fn validate_beacon_block(block: &Vec<u8>, db: &ReamDB) -> anyhow::Result<()> {
    let block =
        SignedBeaconBlock::from_ssz_bytes(&block).map_err(|err| anyhow!(format!("{err:?}")))?;

    let latest_block_in_db = db
        .get_latest_block()
        .map_err(|err| anyhow!(err.to_string()))?;

    ensure!(
        block.message.slot > latest_block_in_db.message.slot,
        "Block slot must be greater than latest block slot in db"
    );

    let start_slot_at_epoch =
        compute_start_slot_at_epoch(db.finalized_checkpoint_provider().get()?.epoch);
    ensure!(
        block.message.slot >= start_slot_at_epoch,
        "Block slot must be greater than start slot at epoch"
    );

    // todo:The block is the first block with valid signature received for the proposer for the
    // slot, signed_beacon_block.message.slot.

    Ok(())
}
