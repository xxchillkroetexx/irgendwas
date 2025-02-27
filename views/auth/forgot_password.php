<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header">Reset Password</div>
            <div class="card-body">
                <p class="card-text">Enter your email address and we will send you a link to reset your password.</p>
                
                <form action="/forgot-password" method="post">
                    <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                    
                    <div class="mb-3">
                        <label for="email" class="form-label">Email Address</label>
                        <input type="email" class="form-control <?= isset($error) ? 'is-invalid' : '' ?>" 
                            id="email" name="email" value="<?= $email ?? '' ?>" required>
                        <?php if (isset($error)): ?>
                            <div class="invalid-feedback"><?= $error ?></div>
                        <?php endif; ?>
                    </div>
                    
                    <div class="d-grid gap-2">
                        <button type="submit" class="btn btn-primary">Send Reset Link</button>
                    </div>
                </form>
            </div>
            <div class="card-footer text-center">
                <p class="mb-0"><a href="/login">Back to Login</a></p>
            </div>
        </div>
    </div>
</div>
