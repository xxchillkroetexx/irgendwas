<?php
/**
 * Forgot Password View
 *
 * Displays a form that allows users to request a password reset link.
 * Users enter their email address to receive a password reset token.
 * 
 * @uses t() Translation function for internationalization
 */
?>
<div class="row justify-content-center">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header bg-primary text-white">
                <h4 class="mb-0"><?= t('auth.forgotPassword.title') ?></h4>
            </div>
            <div class="card-body">
                <!-- Explanatory text -->
                <p class="mb-3"><?= t('auth.forgotPassword.description') ?></p>

                <!-- Password reset request form -->
                <form action="/auth/forgot-password" method="post">
                    <!-- Email input for identifying the account -->
                    <div class="mb-3">
                        <label for="email" class="form-label"><?= t('auth.forgotPassword.email') ?></label>
                        <input type="email" class="form-control" id="email" name="email" required>
                    </div>
                    
                    <!-- Submit button -->
                    <div class="d-grid">
                        <button type="submit" class="btn btn-primary"><?= t('auth.forgotPassword.submit') ?></button>
                    </div>
                </form>

                <!-- Back to login page link -->
                <div class="mt-3 text-center">
                    <a href="/auth/login"><?= t('auth.forgotPassword.backToLogin') ?></a>
                </div>
            </div>
        </div>
    </div>
</div>