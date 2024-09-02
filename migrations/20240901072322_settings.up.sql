-- Add up migration script here
create table IF NOT EXISTS settings
(
    id text default 'DEFAULT_SETTINGS' not null primary key,
    encrypted_global_api_key text not null
)