<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header bg-primary text-white">
                <h4 class="mb-0"><?= t('auth.login.title') ?></h4>
            </div>
            <div class="card-body">
                <form action="/auth/login" method="post">
                    <div class="mb-3">
                        <label for="email" class="form-label"><?= t('auth.login.email') ?></label>
                        <input type="email" class="form-control" id="email" name="email" required>
                    </div>
                    <div class="mb-3">
                        <label for="password" class="form-label"><?= t('auth.login.password') ?></label>
                        <input type="password" class="form-control" id="password" name="password" required>
                    </div>
                    <div class="mb-3 form-check">
                        <input type="checkbox" class="form-check-input" id="remember" name="remember">
                        <label class="form-check-label" for="remember"><?= t('auth.login.remember') ?></label>
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-primary"><?= t('auth.login.submit') ?></button>
                    </div>
                </form>

                <div class="mt-3 text-center">
                    <a href="/auth/forgot-password"><?= t('auth.login.forgotPassword') ?></a>
                </div>

                <hr>

                <div class="text-center">
                    <p><?= t('auth.login.noAccount') ?></p>
                    <a href="/auth/register" class="btn btn-outline-success"><?= t('auth.register') ?></a>
                </div>
            </div>
        </div>
    </div>
</div>