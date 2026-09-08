-- NULL preserves the native default for every existing custom profile.
ALTER TABLE presets ADD COLUMN base_instructions TEXT;
