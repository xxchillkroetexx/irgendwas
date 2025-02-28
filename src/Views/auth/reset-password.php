<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header">
                <h4 class="mb-0"><?= $t('auth.reset_password') ?></h4>
            </div>
            <div class="card-body">
                <form action="/auth/reset-password" method="post">
                    <input type="hidden" name="token" value="<?= $view->escape($token) ?>">
                    
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
                        <button type="submit" class="btn btn-primary"><?= $t('auth.reset_password') ?></button>
                    </div>
                </form>
                
                <div class="mt-3 text-center">
                    <a href="/auth/login">Back to login</a>
                </div>
            </div>
        </div>
    </div>
</div>