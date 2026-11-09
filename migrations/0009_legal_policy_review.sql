ALTER TABLE legal_profile ADD COLUMN privacy_legal_basis TEXT NOT NULL DEFAULT '';
ALTER TABLE legal_profile ADD COLUMN privacy_retention TEXT NOT NULL DEFAULT '';
ALTER TABLE legal_profile ADD COLUMN privacy_transfer_details TEXT NOT NULL DEFAULT '';
ALTER TABLE legal_profile ADD COLUMN policy_published INTEGER NOT NULL DEFAULT 0 CHECK (policy_published IN (0, 1));
