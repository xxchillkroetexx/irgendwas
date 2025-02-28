<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header">
                <h4 class="mb-0"><?= $t('auth.forgot_password') ?></h4>
            </div>
            <div class="card-body">
                <p class="mb-3">Enter your email address and we'll send you a link to reset your password.</p>
                
                <form action="/auth/forgot-password" method="post">
                    <div class="mb-3">
                        <label for="email" class="form-label"><?= $t('auth.email') ?></label>
                        <input type="email" class="form-control" id="email" name="email" required>
                    </div>
                    <div class="d-grid">
                        <button type="submit" class="btn btn-primary"><?= $t('auth.submit') ?></button>
                    </div>
                </form>
                
                <div class="mt-3 text-center">
                    <a href="/auth/login">Back to login</a>
                </div>
            </div>
        </div>
    </div>
</div>