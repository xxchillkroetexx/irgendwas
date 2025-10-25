<?php
/**
 * Group joining view template
 * 
 * This template allows users to join an existing group by entering
 * an invitation code that was shared with them.
 * 
 * @var Session $session Session instance for handling form data persistence
 * @var string|null $code Optional invitation code passed via URL
 */
?>

<!-- Join group form container -->
<div class="row justify-content-center">
    <div class="col-md-8 col-lg-6">
        <div class="card">
            <div class="card-header">
                <h3 class="card-title"><?= t('group.join.title') ?></h3>
            </div>
            <div class="card-body">
                <p class="card-text"><?= t('group.join.description') ?></p>

                <!-- Join group form -->
                <form action="/groups/join" method="post">
                    <div class="mb-3">
                        <label for="invitation_code" class="form-label"><?= t('group.join.code') ?> <span class="text-danger">*</span></label>
                        <input type="text" class="form-control" id="invitation_code" name="invitation_code" required
                            value="<?= htmlspecialchars($session->getOldInput('invitation_code', $code ?? '')) ?>"
                            placeholder="<?= t('group.join.codePlaceholder') ?>">
                        <div class="form-text"><?= t('group.join.codeHelp') ?></div>
                    </div>

                    <!-- Form action buttons -->
                    <div class="d-grid gap-2">
                        <button type="submit" class="btn btn-primary"><?= t('group.join.submit') ?></button>
                        <a href="/groups" class="btn btn-outline-secondary"><?= t('group.join.cancel') ?></a>
                    </div>
                </form>
            </div>
        </div>
    </div>
</div>