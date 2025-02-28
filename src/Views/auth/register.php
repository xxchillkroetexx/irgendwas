<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header">
                <h4 class="mb-0"><?= $t('auth.register') ?></h4>
            </div>
            <div class="card-body">
                <form action="/auth/register" method="post">
                    <div class="mb-3">
                        <label for="name" class="form-label"><?= $t('auth.name') ?></label>
                        <input type="text" class="form-control" id="name" name="name" required>
                    </div>
                    <div class="mb-3">
                        <label for="email" class="form-label"><?= $t('auth.email') ?></label>
                        <input type="email" class="form-control" id="email" name="email" required>
                    </div>
                    <div class="mb-3">
                        <label for="password" class="form-label"><?= $t('auth.password') ?></label>
                        <input type="password" class="form-control" id="password" name="password" minlength="8" required>
                        <div class="form-text">Password must be at least 8 characters long.</div>
                    </div>
                    <div class="mb-3">
                        <label for="password_confirm" class="form-label"><?= $t('auth.confirm_password') ?></label>
                        <input type="password" class="form-control" id="password_confirm" name="password_confirm" minlength="8" required>
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-success"><?= $t('auth.register') ?></button>
                    </div>
                </form>
                
                <hr>
                
                <div class="text-center">
                    <p><?= $t('auth.already_have_account') ?></p>
                    <a href="/auth/login" class="btn btn-outline-primary"><?= $t('auth.login') ?></a>
                </div>
            </div>
        </div>
    </div>
</div>