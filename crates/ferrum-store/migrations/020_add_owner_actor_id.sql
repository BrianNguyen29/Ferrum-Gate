ALTER TABLE intents ADD COLUMN owner_actor_id TEXT;
ALTER TABLE proposals ADD COLUMN owner_actor_id TEXT;
ALTER TABLE capabilities ADD COLUMN owner_actor_id TEXT;
ALTER TABLE executions ADD COLUMN owner_actor_id TEXT;
ALTER TABLE approvals ADD COLUMN owner_actor_id TEXT;
ALTER TABLE quarantine_holds ADD COLUMN owner_actor_id TEXT;
