<?php
/**
 * Reset Password View
 *
 * Displays a form that allows users to set a new password after
 * receiving a password reset link via email.
 * 
 * @uses t() Translation function for internationalization
 * @uses $token The password reset token from the URL
 */
?>
<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header bg-primary text-white">
                <h4 class="mb-0"><?= t('auth.resetPassword.title') ?></h4>
            </div>
            <div class="card-body">
                <!-- Password reset form -->
                <form action="/auth/reset-password" method="post">
                    <!-- Hidden field containing the reset token -->
                    <input type="hidden" name="token" value="<?= htmlspecialchars($token) ?>">

                    <!-- New password input field -->
                    <div class="mb-3">
                        <label for="password" class="form-label"><?= t('auth.resetPassword.newPassword') ?></label>
                        <input type="password" class="form-control" id="password" name="password" required minlength="8">
                        <div class="form-text"><?= t('auth.resetPassword.passwordRequirements') ?></div>
                    </div>
                    
                    <!-- Password confirmation field -->
                    <div class="mb-3">
                        <label for="password_confirm" class="form-label"><?= t('auth.resetPassword.confirmPassword') ?></label>
                        <input type="password" class="form-control" id="password_confirm" name="password_confirm" required minlength="8">
                    </div>
                    
                    <!-- Submit button -->
                    <div class="d-grid">
                        <button type="submit" class="btn btn-primary"><?= t('auth.resetPassword.submit') ?></button>
                    </div>
                </form>

                <!-- Back to login page link -->
                <div class="mt-3 text-center">
                    <a href="/auth/login"><?= t('auth.resetPassword.backToLogin') ?></a>
                </div>
            </div>
        </div>
    </div>
</div>