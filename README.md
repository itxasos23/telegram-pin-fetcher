# Summary

This project logs into telegram on first run.
Once logged in, session is stored in `dialog.session` file.

It will try to fecth creds and config from `.config/telegram_pin_fetcher/cofig.toml` with the following format:

```toml
[telegram_api_creds]
api_id = {api_id} 
api_hash = {api_hash} 

[config]
usernames = [{usernames}]
```

It will serialize all the pinned messages from chats with the shared usernames, store them in out.json file and push it to file.io.

TODO:
- uploader:
  - reformat code (better rust code)
  - run as sysmtectl service (fetch only on startup, check if today file exists, if not, compile and compare. If equal, update, else upload)

- downloader:
  - total todo:
    - verify existing file data
    - if old, 
        - download file (known id from config file)
        - compile sentences

    - if update, do nothing

