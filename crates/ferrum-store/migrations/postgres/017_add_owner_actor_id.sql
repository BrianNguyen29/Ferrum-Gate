ALTER TABLE intents ADD COLUMN IF NOT EXISTS owner_actor_id TEXT;
ALTER TABLE proposals ADD COLUMN IF NOT EXISTS owner_actor_id TEXT;
ALTER TABLE capabilities ADD COLUMN IF NOT EXISTS owner_actor_id TEXT;
ALTER TABLE executions ADD COLUMN IF NOT EXISTS owner_actor_id TEXT;
ALTER TABLE approvals ADD COLUMN IF NOT EXISTS owner_actor_id TEXT;
ALTER TABLE quarantine_holds ADD COLUMN IF NOT EXISTS owner_actor_id TEXT;
