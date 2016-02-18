Request Workflow:
  (cache for x min?)
 -> /github/:user/:repo/:branch.png

 -> pull https://api.github.com/repos/:user/:repo/git/refs/heads/:branch
   -> JSON: object.sha
 -> check for `:user_:repo_:sha`
    -> Found: return
 -> store 'checking' at `:user_:repo_:sha`
 -> fetch https://github.com/:repo/:user/archive/:sha.zip
    -> unzip `builds/:user_:repo_:sha`
    -> run `rustc -Zextra-plugins=clippy -L/builds/:user_:repo_:sha`
    -> collect result
    -> store at `:user_:repo_:sha`
