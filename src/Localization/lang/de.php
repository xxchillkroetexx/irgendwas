<?php

return [
    'app' => [
        'name' => 'Wichteln',
        'tagline' => 'Geschenke-Geben leicht gemacht'
    ],
    'auth' => [
        'login' => 'Anmelden',
        'register' => 'Registrieren',
        'logout' => 'Abmelden',
        'email' => 'E-Mail',
        'password' => 'Passwort',
        'confirm_password' => 'Passwort bestätigen',
        'name' => 'Name',
        'remember_me' => 'Angemeldet bleiben',
        'forgot_password' => 'Passwort vergessen?',
        'reset_password' => 'Passwort zurücksetzen',
        'submit' => 'Absenden',
        'already_have_account' => 'Bereits ein Konto?',
        'dont_have_account' => 'Noch kein Konto?'
    ],
    'groups' => [
        'my_groups' => 'Meine Gruppen',
        'create_group' => 'Gruppe erstellen',
        'join_group' => 'Gruppe beitreten',
        'group_name' => 'Gruppenname',
        'group_description' => 'Gruppenbeschreibung',
        'admin' => 'Administrator',
        'created_at' => 'Erstellt am',
        'registration_deadline' => 'Anmeldefrist',
        'draw_date' => 'Auslosungsdatum',
        'invitation_code' => 'Einladungscode',
        'members' => 'Mitglieder',
        'add_member' => 'Mitglied hinzufügen',
        'remove_member' => 'Mitglied entfernen',
        'perform_draw' => 'Auslosung durchführen',
        'draw_completed' => 'Auslosung abgeschlossen',
        'draw_pending' => 'Auslosung ausstehend',
        'invite_participants' => 'Teilnehmer einladen',
        'participant_email' => 'E-Mail des Teilnehmers',
        'invite' => 'Einladen',
        'view_details' => 'Details anzeigen',
        'edit_group' => 'Gruppe bearbeiten',
        'delete_group' => 'Gruppe löschen',
        'confirm_delete' => 'Sind Sie sicher, dass Sie diese Gruppe löschen möchten?'
    ],
    'wishlists' => [
        'my_wishlist' => 'Meine Wunschliste',
        'view_wishlist' => 'Wunschliste anzeigen',
        'edit_wishlist' => 'Wunschliste bearbeiten',
        'add_item' => 'Wunsch hinzufügen',
        'edit_item' => 'Wunsch bearbeiten',
        'delete_item' => 'Wunsch löschen',
        'item_title' => 'Titel',
        'item_description' => 'Beschreibung',
        'item_link' => 'Link',
        'priority_ordered' => 'Diese Wunschliste ist nach Priorität geordnet',
        'save_changes' => 'Änderungen speichern',
        'move_up' => 'Nach oben',
        'move_down' => 'Nach unten'
    ],
    'assignments' => [
        'your_recipient' => 'Dein Beschenkter',
        'view_wishlist' => 'Wunschliste anzeigen',
        'assignment_date' => 'Zuweisungsdatum'
    ],
    'exclusions' => [
        'manage_exclusions' => 'Ausschlüsse verwalten',
        'add_exclusion' => 'Ausschluss hinzufügen',
        'user' => 'Benutzer',
        'excluded_user' => 'Darf nicht beschenken',
        'delete_exclusion' => 'Ausschluss löschen'
    ],
    'notifications' => [
        'invitation_subject' => 'Du wurdest zu einer Wichtelgruppe eingeladen',
        'draw_subject' => 'Deine Wichtel-Auslosung',
        'password_reset_subject' => 'Passwort-Zurücksetzung angefordert'
    ],
    'errors' => [
        'invalid_credentials' => 'Ungültige E-Mail oder Passwort',
        'email_taken' => 'Diese E-Mail ist bereits registriert',
        'passwords_dont_match' => 'Passwörter stimmen nicht überein',
        'group_not_found' => 'Gruppe nicht gefunden',
        'user_not_found' => 'Benutzer nicht gefunden',
        'wishlist_not_found' => 'Wunschliste nicht gefunden',
        'not_authorized' => 'Du bist nicht berechtigt, diese Aktion durchzuführen',
        'invalid_invitation_code' => 'Ungültiger Einladungscode',
        'already_member' => 'Du bist bereits Mitglied dieser Gruppe',
        'draw_already_completed' => 'Die Auslosung wurde bereits durchgeführt',
        'not_enough_members' => 'Mindestens 2 Mitglieder sind für eine Auslosung erforderlich',
        'no_assignment' => 'Dir wurde noch kein Empfänger zugewiesen'
    ],
    'success' => [
        'login_success' => 'Du hast dich erfolgreich angemeldet',
        'register_success' => 'Du hast dich erfolgreich registriert',
        'logout_success' => 'Du hast dich erfolgreich abgemeldet',
        'group_created' => 'Gruppe wurde erfolgreich erstellt',
        'group_updated' => 'Gruppe wurde erfolgreich aktualisiert',
        'group_deleted' => 'Gruppe wurde erfolgreich gelöscht',
        'joined_group' => 'Du bist der Gruppe erfolgreich beigetreten',
        'invitation_sent' => 'Einladung wurde erfolgreich gesendet',
        'draw_completed' => 'Die Auslosung wurde erfolgreich durchgeführt',
        'wishlist_updated' => 'Wunschliste wurde erfolgreich aktualisiert',
        'password_reset_link_sent' => 'Ein Link zum Zurücksetzen des Passworts wurde an deine E-Mail gesendet',
        'password_reset_success' => 'Dein Passwort wurde erfolgreich zurückgesetzt'
    ]
];