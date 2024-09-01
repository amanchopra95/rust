-- Add up migration script here
create table IF NOT EXISTS link_statistics
(
    id serial primary key,
    link_id text not null,
    referer text, 
    user_agent text,
    constraint fk_links 
        foreign key (link_id)
            references links (id)
);

create index idx_link_statistics_link_id on link_statistics using btree (link_id);