### piki-ctl — open files in piki's UI

If `piki-ctl` is available on PATH, you can ask piki to open a file in its
own file preview instead of just printing the file's contents:

```
piki-ctl open file <path>
```

`<path>` is resolved against your current working directory. Use this when
the user in a piki-managed terminal asks you to "show", "open" or "pull up"
a file. If the command isn't found, piki-ctl simply isn't installed in this
environment — fall back to your normal behavior without retrying.
