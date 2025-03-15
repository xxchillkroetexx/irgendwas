<?php $this->layout('layouts/app', ['title' => 'Dashboard']) ?>

<div class="container mt-4">
    <div class="row">
        <div class="col-md-12">
            <h1>Willkommen, <?= $this->e($user->getName()) ?>!</h1>
            
            <?php if ($this->flash()->has('warning')): ?>
                <div class="alert alert-warning">
                    <?= $this->flash()->get('warning') ?>
                </div>
            <?php endif; ?>
            
            <?php if ($this->flash()->has('success')): ?>
                <div class="alert alert-success">
                    <?= $this->flash()->get('success') ?>
                </div>
            <?php endif; ?>
            
            <!-- Rest des Dashboard-Inhalts -->
            <div class="row mt-4">
                <div class="col-md-6">
                    <div class="card">
                        <div class="card-header bg-primary text-white">
                            <h5 class="mb-0">Meine Gruppen</h5>
                        </div>
                        <div class="card-body">
                            <?php if (empty($groups)): ?>
                                <p>Sie sind noch kein Mitglied einer Wichtelgruppe.</p>
                                <a href="/groups/join" class="btn btn-primary">Einer Gruppe beitreten</a>
                                <a href="/groups/create" class="btn btn-outline-primary">Neue Gruppe erstellen</a>
                            <?php else: ?>
                                <ul class="list-group">
                                    <?php foreach ($groups as $group): ?>
                                        <li class="list-group-item d-flex justify-content-between align-items-center">
                                            <a href="/groups/view/<?= $group->getId() ?>"><?= $this->e($group->getName()) ?></a>
                                            <span class="badge bg-primary rounded-pill"><?= count($group->getMembers() ?? []) ?> Mitglieder</span>
                                        </li>
                                    <?php endforeach; ?>
                                </ul>
                                <div class="mt-3">
                                    <a href="/groups/join" class="btn btn-sm btn-primary">Einer weiteren Gruppe beitreten</a>
                                </div>
                            <?php endif; ?>
                        </div>
                    </div>
                </div>
                
                <div class="col-md-6">
                    <div class="card">
                        <div class="card-header bg-success text-white">
                            <h5 class="mb-0">Meine Zuweisungen</h5>
                        </div>
                        <div class="card-body">
                            <?php if (empty($assignments)): ?>
                                <p>Sie haben noch keine Wichtel-Zuweisungen.</p>
                            <?php else: ?>
                                <ul class="list-group">
                                    <?php foreach ($assignments as $assignment): ?>
                                        <li class="list-group-item">
                                            In <strong><?= $this->e($assignment->getGroup()->getName()) ?></strong> beschenken Sie: 
                                            <strong><?= $this->e($assignment->getReceiver()->getName()) ?></strong>
                                            <a href="/wishlist/view/<?= $assignment->getReceiver()->getId() ?>/<?= $assignment->getGroupId() ?>" 
                                               class="btn btn-sm btn-outline-success float-end">Wunschliste ansehen</a>
                                        </li>
                                    <?php endforeach; ?>
                                </ul>
                            <?php endif; ?>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>
</div>
